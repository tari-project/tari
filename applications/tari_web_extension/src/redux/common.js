/*global chrome*/
export function sendMessagePromise(message) {
  return new Promise((resolve, reject) => {
    if (chrome.runtime) {
      chrome.runtime.sendMessage(message, (response) => {
        if (response?.successful) resolve(response);
        reject();
      });
    } else {
      // Mock for testing UI in non-extension version.
      // TODO: Delete this before any release
      console.log("mock", message.action);
      switch (message.action) {
        case "tari-get-assets":
          resolve({ successful: true, assets: ["asset1", "asset2", "asset3"] });
          break;
        case "tari-login":
          resolve({ successful: true, token: "token" });
          break;
        case "tari-select-asset":
          resolve({ successful: true, selected: "asset2" });
          break;
        case "tari-get-selected-asset":
          resolve({ successful: true, selected: "asset2" });
          break;
        default:
          console.log("unimplemented mock for", message);
          break;
      }
    }
  });
}
