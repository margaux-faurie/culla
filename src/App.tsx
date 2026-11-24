import { useEffect, useState } from "react";
import { useBackend } from "./backend/context";

export function App() {
  const backend = useBackend();
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    backend.version().then(
      (value) => {
        if (active) {
          setVersion(value);
        }
      },
      () => {
        if (active) {
          setVersion("unknown");
        }
      },
    );
    return () => {
      active = false;
    };
  }, [backend]);

  return (
    <main className="start">
      <h1 className="title">Culla</h1>
      <p className="lead">Not ready yet</p>
      <p className="small">{version === null ? "Loading" : `Version ${version}`}</p>
    </main>
  );
}
