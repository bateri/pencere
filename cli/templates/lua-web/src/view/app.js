// The page of {{name}}, written against the bridge of a page: `call` asks the program for
// something once, `subscribe` listens to a topic of its, `onContext` is the terminal's own word
// about itself (its theme, font and size). There is no build step and no other library.
import { call, onContext, subscribe } from "@pencere/sdk/page";

onContext((context) => {
  document.documentElement.style.colorScheme = context.mode;
  document.body.style.background = context.colors.background;
  document.body.style.color = context.colors.foreground;
  if (context.font) document.body.style.fontFamily = context.font;
});

subscribe("clock", undefined, {
  onPush: ({ now }) => {
    document.getElementById("clock").textContent =
      new Date(now * 1000).toUTCString().slice(17, 25) + " UTC";
  },
});

document.getElementById("greet").addEventListener("click", async () => {
  const greeting = document.getElementById("greeting");
  try {
    const { message } = await call("hello", { name: "pencere" });
    greeting.textContent = message;
  } catch (error) {
    greeting.textContent = `It failed: ${error.message ?? error.code}`;
  }
});
