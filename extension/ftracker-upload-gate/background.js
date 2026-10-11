const HOST = "com.ftracker.host";
const MAX_MB = 100;

chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
  if (!msg || msg.type !== "ftracker_upload_gate") {
    return;
  }
  const size = Number(msg.size) || 0;
  if (size <= MAX_MB * 1024 * 1024) {
    sendResponse({ allow: true });
    return true;
  }
  chrome.runtime.sendNativeMessage(
    HOST,
    {
      type: "upload_gate",
      name: String(msg.name || "file"),
      size,
      site: String(msg.site || ""),
      browser: "chrome.exe",
    },
    (response) => {
      if (chrome.runtime.lastError) {
        sendResponse({ allow: false });
        return;
      }
      sendResponse({ allow: !!(response && response.allow) });
    }
  );
  return true;
});
