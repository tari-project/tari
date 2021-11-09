console.log("content.js executed");

function script() {
  // Injected script into the website, to add window.tari
  let promises = {};
  function createTimeoutPromise(payload, timeout_ms = 1000) {
    let id = Math.random();
    const timeout = new Promise((_resolve, reject) =>
      setTimeout(() => {
        delete promises[id];
        reject("timed out");
      }, timeout_ms)
    );
    const response = new Promise((resolve, reject) => {
      promises[id] = { resolve, reject };
      window.postMessage({
        id: id,
        ...payload,
      });
    });
    return Promise.race([timeout, response]);
  }

  class Tari {
    constructor() {
      console.log("initiating tari");
    }
    getSelectedAsset(timeout_ms = 1000) {
      return createTimeoutPromise(
        { action: "tari-get-selected-asset" },
        timeout_ms
      );
    }
  }

  window.addEventListener("message", (event) => {
    if (
      event?.data?.action === "tari-background-response" &&
      promises?.[event?.data?.id]
    ) {
      promises[event.data.id].resolve(event?.data?.selected);
      delete promises[event.data.id];
    }
  });
  window.tari = new Tari();
  console.log("injecting tari");
  // The end of injected script
}

function inject(fn) {
  const scriptContent = document.createElement("script");
  scriptContent.text = `(${fn.toString()})();`;
  document.documentElement.appendChild(scriptContent);
}

inject(script);

let port = chrome.runtime.connect({ name: "tari-port" });
window.addEventListener(
  // Receive message from injected script and resend it to the background.js
  "message",
  (event) => {
    if (event.source != window) {
      return;
    }
    if (
      event?.data?.action?.startsWith("tari-") &&
      event?.data?.action !== "tari-background-response"
    ) {
      port.postMessage(event.data);
    }
  },
  false
);

port.onMessage.addListener(function (payload) {
  // Receive message from background.js and resend to the injected script
  window.postMessage({
    action: "tari-background-response",
    ...payload,
  });
});
