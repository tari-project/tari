console.log("background.js execute");

let selectedAsset = undefined;
let accounts = []; // Accounts registered
let triggered_port = []; // All the triggered ports
let connecting = {}; // Callbacks for the connect requests

// I'm not sure if this changes yet
const extension_url = "chrome-extension://kljpkkadbelknhmpabjkkhmljaebnopo";

let allowedOrigins = {
  [extension_url]: true,
};

const createAccount = (origin) => {
  const account = "0xDEADBEEF";
  accounts = [account];
  return { successful: true, accounts };
};

const connect = (origin, sendResponse) => {
  // This will popup a connect request, we store the callback
  let callback_id = Math.random();
  connecting[callback_id] = { origin, sendResponse };
  window.open(
    `${extension_url}/index.html#/connecting/${btoa(origin)}/${callback_id}`,
    "_blank",
    "width=400,height=600,titlebar=no,status=no,scrollbars=no,resizable=no,menubar=no,toolbar=no"
  );
};

const connectSite = (origin, callback_id) => {
  // Call the stored callback from connect function
  connecting?.[callback_id]?.sendResponse?.({
    successful: true,
    payload: 0xdeadbeef,
  });
  // Add origin to allowed urls
  const original_origin = connecting?.[callback_id]?.origin;
  console.log("adding to allowed origins", original_origin);
  if (original_origin) {
    allowedOrigins[original_origin] = true;
  }
  triggered_port.forEach(({ port, origin }) => {
    // We send it only to the port that belongs to this origin
    if (origin === original_origin) {
      port.postMessage({
        event: "accountChange",
        payload: { accounts },
      });
    }
  });
};

const getConnectedSites = (origin) => {
  if (origin in allowedOrigins) {
    return {
      successful: true,
      // We don't want to expose the permission for the extension itself
      sites: Object.keys(allowedOrigins)
        .filter((site) => site !== extension_url)
        .reduce(
          (filtered, site) => ({ ...filtered, [site]: allowedOrigins[site] }),
          {}
        ),
    };
  }
  return { successful: false, sites: {} };
};

const getAccounts = (origin) => {
  if (origin in allowedOrigins)
    return { successful: true, payload: { accounts } };
  else return { successful: false, payload: { accounts: [] } };
};

const getAssets = (origin) => ({
  successful: true,
  assets: ["asset1", "asset2"],
  selected: selectedAsset,
});

const selectAsset = (origin, request) => {
  selectedAsset = request.name;
  return { successful: true, payload: { selected: selectedAsset } };
};

const getSelectedAsset = (origin) => ({
  successful: true,
  payload: { selected: selectedAsset },
});

const getSeedWords = (origin) => ({
  successful: true,
  payload: {
    seedWords:
      "theme panther ladder custom field aspect misery shine bundle worry senior velvet brush tourist glide jump example vanish embody enemy struggle air extend empty",
  },
});

function messageCallback(request, sender, sendResponse) {
  const { origin } = sender;
  console.log("messageCallback", request, origin);
  switch (request?.action) {
    case "tari-create-account":
      sendResponse(createAccount(origin));
      break;
    case "tari-connect": // Connecting app to the web extension
      connect(origin, sendResponse);
      break;
    case "tari-connect-site":
      connectSite(origin, request?.callback_id);
      break;
    case "tari-get-connected-sites":
      sendResponse(getConnectedSites(origin));
      break;
    case "tari-get-accounts":
      sendResponse(getAccounts(origin));
      break;
    case "tari-get-assets":
      sendResponse(getAssets(origin));
      break;
    case "tari-select-asset":
      sendResponse(selectAsset(origin, request));
      break;
    case "tari-get-selected-asset":
      sendResponse(getSelectedAsset(origin));
      break;
    case "tari-login-refresh":
      sendResponse(loginRefresh(origin));
      break;
    case "tari-get-seedwords":
      sendResponse(getSeedWords(origin));
      break;
    default:
      console.log("unknown message", request?.action);
      break;
  }
}

chrome.runtime.onConnect.addListener(function (port) {
  if (port.name === "tari-port-injected") {
    // We received message from the content.js script.
    port.onMessage.addListener(function ({ data, origin }) {
      messageCallback(data, { origin: origin }, (response) => {
        port.postMessage({
          ...response,
          id: data.id,
        });
      });
    });
  } else if (port.name === "tari-port-triggered") {
    port.onDisconnect.addListener(function (delete_port) {
      triggered_port = triggered_port.filter(
        ({ port, _origin }) => port !== delete_port
      );
    });
    port.onMessage.addListener(function (origin) {
      // Add ports once we know it origin
      triggered_port.push({ port, origin });
    });
  }
});

chrome.runtime.onMessage.addListener(messageCallback);
