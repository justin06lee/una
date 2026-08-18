import { mount } from "svelte";
import Hud from "./Hud.svelte";
import "./hud.css";

mount(Hud, { target: document.getElementById("app")! });
