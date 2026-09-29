import { mount } from "svelte";
import Review from "./Review.svelte";
import "./review.css";

mount(Review, { target: document.getElementById("app")! });
