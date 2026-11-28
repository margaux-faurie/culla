// Types for outside.mjs, so capture.spec.ts can use it under the repository's strict TypeScript.
export declare const REPO: string;
export declare function realPath(path: string): string;
export declare function isInside(path: string, root?: string): boolean;
export declare function assertOutside(path: string, what: string): string;
export declare function assertPlainName(name: string, what: string): string;
