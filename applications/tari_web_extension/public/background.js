console.log("background.js execute");

let selectedAsset = undefined;
let credentials = undefined;

const login = () => ({ successful: true, token: "token" });

const getAssets = () => ({
  successful: true,
  assets: ["asset1", "asset2"],
  selected: selectedAsset,
});

const selectAsset = (request) => {
  selectedAsset = request.name;
  return { successful: true, selected: selectedAsset };
};

const getSelectedAsset = () => ({ successful: true, selected: selectedAsset });

const loginRefresh = () => ({ successful: !!credentials, token: credentials });

function messageCallback(request, sender, sendResponse) {
  switch (request?.action) {
    case "tari-login":
      credentials = "token";
      sendResponse(login());
      break;
    case "tari-get-assets":
      sendResponse(getAssets());
      break;
    case "tari-select-asset":
      sendResponse(selectAsset(request));
      break;
    case "tari-get-selected-asset":
      sendResponse(getSelectedAsset());
      break;
    case "tari-login-refresh":
      sendResponse(loginRefresh());
      break;
    default:
      console.log("unknown message", request?.action);
      break;
  }
}

chrome.runtime.onConnect.addListener(function (port) {
  if (port.name === "tari-port") {
    port.onMessage.addListener(function (data) {
      messageCallback(data, {}, (response) => {
        port.postMessage({
          ...response,
          id: data.id,
        });
      });
    });
  }
});

chrome.runtime.onMessage.addListener(messageCallback);
