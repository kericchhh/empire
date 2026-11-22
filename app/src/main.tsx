import { useState } from "react";
import { createRoot } from "react-dom/client";
import { Opened } from "./api";
import { Home } from "./Home";
import { WorldView } from "./World";
import "@fontsource/jetbrains-mono/400.css";
import "./style.css";

function App() {
  const [world, setWorld] = useState<Opened | null>(null);
  return world ? <WorldView opened={world} onClose={() => setWorld(null)} /> : <Home onOpen={setWorld} />;
}

// A desktop app, not a page: no browser context menu outside text fields.
document.addEventListener("contextmenu", (e) => {
  if (!(e.target as Element).closest("input, textarea")) e.preventDefault();
});

createRoot(document.getElementById("root")!).render(<App />);
