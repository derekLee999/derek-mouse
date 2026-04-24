import { createApp } from "vue";
import ElementPlus from "element-plus";
import "element-plus/dist/index.css";
import "element-plus/theme-chalk/dark/css-vars.css";
import App from "./App.vue";

type ThemeMode = "light" | "dark" | "system";

const systemDarkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function getThemeMode(): ThemeMode {
  const saved = localStorage.getItem("app-settings");
  if (!saved) return "system";

  try {
    const settings = JSON.parse(saved);
    return ["light", "dark", "system"].includes(settings.themeMode)
      ? settings.themeMode
      : "system";
  } catch {
    return "system";
  }
}

function applyTheme() {
  const themeMode = getThemeMode();
  const dark = themeMode === "dark" || (themeMode === "system" && systemDarkQuery.matches);
  document.documentElement.classList.toggle("dark", dark);
}

applyTheme();
systemDarkQuery.addEventListener("change", applyTheme);
window.addEventListener("storage", (event) => {
  if (event.key === "app-settings") {
    applyTheme();
  }
});

createApp(App).use(ElementPlus).mount("#app");
