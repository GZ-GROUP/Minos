import React from "react";
import ReactDOM from "react-dom/client";
import "@fontsource-variable/quicksand";
import "@fontsource-variable/dm-sans";
import "./styles/tokens.css";
import "./styles/global.css";
import "./i18n";
import { IconContext } from "@phosphor-icons/react";
import { ThemeProvider } from "./theme/ThemeProvider";
import App from "./App";

// Icons inherit `currentColor`, so each surface sets their color via the --color-icon token.
const iconDefaults = { weight: "duotone", size: "1em" } as const;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <IconContext.Provider value={iconDefaults}>
      <ThemeProvider>
        <App />
      </ThemeProvider>
    </IconContext.Provider>
  </React.StrictMode>,
);
