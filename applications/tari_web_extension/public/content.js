console.log("content.js executed");

function script() {
  // Injected script into the website, to add window.tari
  let promises = {};

  function createPromise(payload, id) {
    return new Promise((resolve, reject) => {
      promises[id] = { resolve, reject };
      window.postMessage({
        id: id,
        ...payload,
      });
    });
  }

  function createTimeoutPromise(payload, timeout_ms = 1000) {
    let id = Math.random();
    const timeout = new Promise((_resolve, reject) =>
      setTimeout(() => {
        delete promises[id];
        reject("timed out");
      }, timeout_ms)
    );
    return Promise.race([timeout, createPromise(payload, id)]);
  }

  class Tari {
    constructor() {
      console.log("initiating tari");
      this.listeners = {};
    }
    connect() {
      return createPromise({ action: "tari-connect" });
    }
    checkAccounts() {
      return createTimeoutPromise({ action: "tari-get-accounts" });
    }
    getSelectedAsset(timeout_ms = 1000) {
      return createTimeoutPromise(
        { action: "tari-get-selected-asset" },
        timeout_ms
      );
    }
    on(event, listener) {
      // Add listener for background events.
      if (!(event in this.listeners)) {
        this.listeners[event] = listener;
      }
    }
    triggerEvent(event, payload) {
      // Trigger event from background process.
      console.log("triggering event", event, "with payload", payload);
      this.listeners[event]?.(payload);
    }
  }

  window.tari = new Tari();

  const handleBackgroundResponse = (data) => {
    if (promises?.[data?.id]) {
      console.log("handling response", data);
      promises[data.id].resolve(data.payload);
      delete promises[data.id];
    }
  };
  const handleBackgroundEvent = ({ event, payload }) => {
    window.tari.triggerEvent(event, payload);
  };

  window.addEventListener("message", (event) => {
    switch (event?.data?.action) {
      case "tari-background-response":
        handleBackgroundResponse(event?.data?.payload);
        break;
      case "tari-background-event":
        handleBackgroundEvent(event?.data?.payload);
        break;
    }
  });
  console.log("injecting tari");
  // The end of injected script
}

function inject(fn) {
  const scriptContent = document.createElement("script");
  scriptContent.text = `(${fn.toString()})();`;
  document.documentElement.appendChild(scriptContent);
}

inject(script);

// We open two ports. One is for function calls from the injected script
let portInjected = chrome.runtime.connect({ name: "tari-port-injected" });
// The other is from event triggered on the background script
let portTriggered = chrome.runtime.connect({ name: "tari-port-triggered" });
// We need to tell background.js where is this port coming from
portTriggered.postMessage(window.location.origin);

// This listeners is catching message from the injected script and from content.js as well.
// So we need to check what types of message is it.
window.addEventListener(
  "message",
  (event) => {
    if (event.source != window) {
      return;
    }
    if (
      event?.data?.action?.startsWith("tari-") &&
      event?.data?.action !== "tari-background-event" && // tari-background-event and tari-background-response ..
      event?.data?.action !== "tari-background-response" // .. are handled in the injected script
    ) {
      // If we received a tari-message, and it's not response message, send it to the background.js
      portInjected.postMessage({ data: event.data, origin: event.origin });
    }
  },
  false
);

portInjected.onMessage.addListener(function (payload) {
  // Receive message from background.js and resend to the injected script
  console.log("Received from background", payload);
  window.postMessage({
    action: "tari-background-response",
    payload,
  });
});

portTriggered.onMessage.addListener(function (payload) {
  window.postMessage({ action: "tari-background-event", payload });
});
