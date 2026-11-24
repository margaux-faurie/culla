# CI scripts

| Script | What it does |
|---|---|
| `check-conflict-markers.sh` | Fails if a tracked file contains merge-conflict markers. |
| `check_lint_allows.py` | Fails if the removal lint or `unsafe_code` is switched off outside the permitted places. |
| `check-core-deps.sh` | Fails if `culla-core` gains an I/O or Tauri dependency. |
| `vhdx.ps1` | Creates, mounts, detaches, locks and unmounts VHDX test volumes on Windows ([testing.md](../../docs/testing.md) section 5.1). |

## VHDX helpers (`vhdx.ps1`)

Windows integration tests need real second volumes with their own file systems and serials. `vhdx.ps1` makes them with `diskpart`, so there is no dependency on the Hyper-V `New-VHD` cmdlet. It needs an elevated shell; GitHub-hosted Windows runners are elevated. Every native call (`diskpart`, `mountvol`, `fsutil`) runs with a timeout (`-TimeoutSec`, default 60 s), so a blocked detach is killed instead of hanging the job.

```powershell
pwsh -File scripts/ci/vhdx.ps1 -Action mount    -Path C:\tmp\t.vhdx -SizeMB 256 -FileSystem exFAT -Letter T
pwsh -File scripts/ci/vhdx.ps1 -Action detach   -Path C:\tmp\t.vhdx -Letter T            # -Method auto|diskpart|mountvol|dismountdiskimage
pwsh -File scripts/ci/vhdx.ps1 -Action attach   -Path C:\tmp\t.vhdx -Letter W            # reattach, optionally on another letter
pwsh -File scripts/ci/vhdx.ps1 -Action readonly -Path C:\tmp\p.vhdx -Letter P -Method disk # volume|disk|vdisk
pwsh -File scripts/ci/vhdx.ps1 -Action unmount  -Path C:\tmp\t.vhdx
```

| Action | How |
|---|---|
| `mount` | `create vdisk file=... maximum=<SizeMB> type=expandable`, `attach vdisk`, `create partition primary` (MBR), `format fs=<fs> quick label=CULLA_<L>`, `assign letter=<L>`. Refuses an existing `.vhdx` file and a letter in use, then checks that `Get-Volume` reports the requested file system. |
| `attach` | `attach vdisk`, then puts the volume on `-Letter` with `mountvol` (moving it off the letter the mount manager remembered). If the vdisk is still attached (after a `mountvol` detach), it only assigns the mount point, which brings the volume back online. |
| `detach` | Refuses the system drive and any letter not held by the VHDX at `-Path` (checked again before `mountvol /p`; `readonly` has the same guard). `-Method auto` tries, in order, `diskpart detach vdisk`, `mountvol <L>: /p`, `Dismount-DiskImage`, and stops at the first after which the volume root is gone. The result line records every attempt as `method:status/gone-or-present/seconds`. |
| `readonly` | `volume`: `attributes volume set readonly`, then `fsutil volume dismount` so the file system remounts read-only. `disk`: `attributes disk set readonly` on the VHDX's disk, then the same dismount. `vdisk`: `detach vdisk`, then `attach vdisk readonly`. Then `fsutil fsinfo volumeinfo` and records `fsutil_readonly=yes|no`. |
| `unmount` | `detach vdisk` if attached (falls back to `Dismount-DiskImage`, also with a timeout). The `.vhdx` file is left in place. |

With `-ResultFile`, each action appends one line such as `action=detach attempts=diskpart:ok/gone/0.8s letter=T method=diskpart outcome=ok seconds=0.8`. Exit code is 0 on success and 1 on failure.

Limits:
- Admin rights are required (diskpart, mountvol, fsutil). Locally, run from an elevated PowerShell.
- `diskpart format fs=fat32` stops at 32 GB; the script refuses FAT32 below 64 MB. CI uses 64 to 256 MB volumes.
- A detach that times out is killed; diskpart's state after that is not defined, which is why `auto` moves on to the next method.
- On MBR disks, diskpart applies the volume read-only attribute to every volume on the disk. Each VHDX here has one volume, so this does not matter.

## VHDX spike (M0)

The spike proves on `windows-latest` that the card-pull and SD-lock cases of [testing.md](../../docs/testing.md) sections 5 and 6 can run on a hosted runner. `.github/workflows/vhdx-spike.yml` runs on pull requests that touch `vhdx.ps1`, `crates/adapters/tests/vhdx_spike.rs` or the workflow, and on `workflow_dispatch`.

| Step | Volumes |
|---|---|
| Mount | `T:` exFAT 256 MB (card), `U:` NTFS 64 MB, `V:` FAT32 128 MB, `O:` `P:` `Q:` exFAT 64 MB (one per read-only method) |
| Placeholder write | `vhdx_write_placeholder` on each: `create_new`, 1 MiB, `sync_all`, read back, list |
| Detach | `vhdx_detach_spike` opens a read-write handle on `T:` (256 KiB synced, then 64 KiB unsynced) and a read-only handle on a second file, creates `ready` in the signal folder, and waits. The workflow detaches `T:` with `-Method auto`, writes `detached` with the method, and the test records the result of each access through the old handles and through paths: `io::ErrorKind`, raw Win32 code and its name, and duration. |
| Reattach | `T.vhdx` attached again on `W:`; the held file is read back and its synced head and unsynced tail are compared byte for byte with the pattern the test wrote. |
| Read-only | `readonly -Method volume` on `O:`, `disk` on `P:`, `vdisk` on `Q:`, each followed by `fsutil fsinfo volumeinfo` (is the read-only flag reported?), then `vhdx_readonly_spike`. It expects create, mkdir and write to fail; open for write succeeds. It fails if no file from the placeholder step is found. |
| Unmount | Every VHDX detached. |

Every PR also runs a short version in the `rust-windows` job of `ci.yml`: mount one 64 MB exFAT VHDX on `T:`, run `vhdx_write_placeholder` on it, unmount.

The tests in `crates/adapters/tests/vhdx_spike.rs` are `#[ignore]` and return early unless `CULLA_VHDX_LETTER` is set and the OS is Windows, so `cargo test --workspace` and `cargo test -- --ignored` are unaffected elsewhere. They create files only with `create_new` under unique names. The job summary has a table of every step, the test observations and the script result lines.

### Results

Runs, both 24 Nov 2026: [45351286023](https://github.com/margaux-faurie/culla/actions/runs/45351286023) (first run; it ended red on an open-for-write assertion that was later dropped), [45351794930](https://github.com/margaux-faurie/culla/actions/runs/45351794930) (head 3be4214, first run with the read-only write probe) and [45352688496](https://github.com/margaux-faurie/culla/actions/runs/45352688496) (head ba966df, fsutil flag and byte-for-byte checks). Each row cites its run. Runner: image `win25-vs2026` 20261116.315 (Windows Server 2025 Datacenter, build 10.0.26100).

| Question | Result |
|---|---|
| exFAT, FAT32 and NTFS mount at fixed letters | ok, about 2 s each (6 volumes) (45351286023) |
| Placeholder writes on each | ok (1 MiB write + sync + read back, ≤ 2 ms) (45351286023) |
| Detach with handles open: method that made `T:` vanish | `diskpart detach vdisk`, 1.4 s (45351286023), 1.7 s (45351794930) and 1.8 s (45352688496); no fallback needed |
| `diskpart detach vdisk` with handles open: refused, blocked or done | done (45351286023) |
| Write through held handle after detach | fails at once: `ERROR_NOT_READY` (21) (45351286023) |
| `sync_all` through held handle after detach | fails at once: `ERROR_NOT_READY` (21) (45351286023) |
| Read through held handles after detach | fails at once: `ERROR_NOT_READY` (21), read-write and read-only handles (45351286023) |
| Path access after detach (`metadata`, open, `create_new`, `read_dir`) | `ERROR_PATH_NOT_FOUND` (3) (45351286023) |
| Closing held handles after detach | returns at once (45351286023) |
| Reattach on another letter, held file length | ok; 327680, so the unsynced 64 KiB tail survived on this virtual disk (a real card may lose it) (45351286023). Byte for byte (45352688496): synced head matches, unsynced tail matches, 0 bytes past the tail |
| Read-only: `volume` attribute | creating files and folders fails (`ERROR_WRITE_PROTECT`, 19; 45351286023); opening an existing file for writing still succeeds, and the write fails with 19 (45351794930). `fsutil` read-only flag: yes, `Is ReadOnly` (45352688496) |
| Read-only: `disk` attribute | same as `volume` (45351286023, 45351794930, 45352688496). `fsutil` read-only flag: yes, `Is ReadOnly` (45352688496) |
| Read-only: `attach vdisk readonly` | same as `volume` (45351286023, 45351794930, 45352688496). `fsutil` read-only flag: yes, `Is ReadOnly` (45352688496) |
| Unmount | ok (45351286023) |

Method to use for the card-pull test (testing.md section 6, level 4): `diskpart detach vdisk` while the test holds handles; expect `ERROR_NOT_READY` on handles and `ERROR_PATH_NOT_FOUND` on paths.
Method to use for the write-protect case: `attributes disk set readonly` (`-Method disk`): `fsutil fsinfo volumeinfo` reports `Is ReadOnly` for it (45352688496), and it write-protects the whole disk, which is closer to a real SD card's lock switch than a volume flag. The P3 preflight uses the volume read-only flag either way, and writes must still handle `ERROR_WRITE_PROTECT` (19). Note for the engine: an SD lock is not detected by opening a file for writing; only the write fails.
