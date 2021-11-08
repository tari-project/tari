console.log("content.js executed");

function script() {
  class Tari {
    constructor() {
      console.log("initiating tari");
    }
    login() {
      window.postMessage(
        { type: "TARI", payload: { user: "username", password: "password" } },
        "*"
      );
    }
  }
  window.tari = new Tari();
  console.log("injecting tari");
}

function inject(fn) {
  const script = document.createElement("script");
  script.text = `(${fn.toString()})();`;
  document.documentElement.appendChild(script);
}

inject(script);

let port = chrome.runtime.connect({ name: "tari-port" });
window.addEventListener(
  "message",
  (event) => {
    if (event.source != window) {
      return;
    }
    if (event.data.type && event.data.type == "TARI") {
      console.log("content received : ", event.data.payload);
      port.postMessage(event.data.payload);
    }
  },
  false
);
