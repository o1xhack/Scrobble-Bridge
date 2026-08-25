import "@scrobble-bridge/ui/theme.css";
import "./app.css";
import App from "./App.svelte";
import { mount } from "svelte";

mount(App, { target: document.getElementById("app")! });
