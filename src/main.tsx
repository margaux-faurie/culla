import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { createBackend } from "#backend-impl";
import { App } from "./App";
import { BackendContext } from "./backend/context";
import "./styles/app.css";

const root = document.getElementById("root");
if (root === null) {
  throw new Error("index.html has no #root element");
}

const backend = createBackend();

createRoot(root).render(
  <StrictMode>
    <BackendContext value={backend}>
      <App />
    </BackendContext>
  </StrictMode>,
);
