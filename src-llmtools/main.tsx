import "../src/styles.css";
import ReactDOM from "react-dom/client";
import { HeroUIProvider } from "@heroui/react";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <HeroUIProvider>
    <App />
  </HeroUIProvider>
);
