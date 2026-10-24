// The view: a web page. It reaches its program only through the bridge the terminal gives a page
// (`window.parent.postMessage` with single-member objects, the specification's §9.4); the page
// module of the SDK, `@pencere/sdk/page`, is that as functions, and a bundle can carry it.
let next = 1;
const asked = new Map();

function say(message) {
  window.parent.postMessage(message, "*");
}

function call(method, params) {
  const id = next++;
  return new Promise((resolve, reject) => {
    asked.set(id, { resolve, reject });
    say({ call: { id, method, params } });
  });
}

function subscribe(topic, onPush) {
  const id = next++;
  asked.set(id, { push: onPush });
  say({ subscribe: { id, topic } });
}

window.addEventListener("message", (event) => {
  const message = event.data ?? {};
  if (message.response) {
    const wait = asked.get(message.response.id);
    asked.delete(message.response.id);
    if (message.response.error) wait?.reject(message.response.error);
    else wait?.resolve(message.response.result);
  } else if (message.push) {
    asked.get(message.push.id)?.push?.(message.push.value);
  }
});

subscribe("clock", ({ now }) => {
  document.getElementById("clock").textContent =
    new Date(now * 1000).toUTCString().slice(17, 25) + " UTC";
});

document.getElementById("greet").addEventListener("click", async () => {
  try {
    const { message } = await call("hello", { name: "pencere" });
    document.getElementById("greeting").textContent = message;
  } catch (error) {
    document.getElementById("greeting").textContent = `It failed: ${error.message ?? error.code}`;
  }
});
