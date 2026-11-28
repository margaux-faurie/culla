import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { InstallPage } from "./InstallPage";
import "../styles/app.css";

// Entry of install.html, built in the demo only (vite.config.ts).
const root = document.getElementById("root");
if (root === null) {
  throw new Error("install.html has no #root element");
}

createRoot(root).render(
  <StrictMode>
    <InstallPage />
  </StrictMode>,
);
