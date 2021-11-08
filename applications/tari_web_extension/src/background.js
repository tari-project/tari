console.log("background.js execute");
chrome.runtime.onConnect.addListener(function (port) {
  if (port.name === "tari-port") {
    port.onMessage.addListener(function (payload) {
      console.log(payload);
    });
  }
});
