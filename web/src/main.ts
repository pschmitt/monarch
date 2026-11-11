import "./app.css";
import "@fontsource-variable/bricolage-grotesque";
import "@fontsource-variable/source-sans-3";
import "./character.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { initPwa } from "./lib/pwa.svelte";

initPwa();
mount(App, { target: document.getElementById("app")! });
