import { createRoot } from "react-dom/client";
import App from "./App";
import { LocaleProvider } from "./locale";
import "./style.css";
createRoot(document.getElementById("root")!).render(
  <LocaleProvider>
    <App />
  </LocaleProvider>,
);
