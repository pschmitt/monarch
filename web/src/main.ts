import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";
import { initPwa } from "./lib/pwa.svelte";

initPwa();
mount(App, { target: document.getElementById("app")! });
