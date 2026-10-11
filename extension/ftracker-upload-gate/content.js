(function () {
  const MAX = 100 * 1024 * 1024;

  function tooLarge(files) {
    if (!files || !files.length) return null;
    for (let i = 0; i < files.length; i += 1) {
      const f = files[i];
      if (f && f.size > MAX) return f;
    }
    return null;
  }

  function gate(files, input) {
    const big = tooLarge(files);
    if (!big) return false;
    try {
      chrome.runtime.sendMessage(
        {
          type: "ftracker_upload_gate",
          name: big.name,
          size: big.size,
          site: location.hostname,
        },
        function (resp) {
          if (resp && resp.allow && input) {
            // File handle was cleared; user must pick again after allow.
          }
        }
      );
    } catch (_e) {
      /* extension context */
    }
    if (input) {
      try {
        input.value = "";
      } catch (_e2) {
        /* ignore */
      }
    }
    return true;
  }

  document.addEventListener(
    "change",
    function (ev) {
      const t = ev.target;
      if (!t || t.tagName !== "INPUT" || String(t.type).toLowerCase() !== "file") return;
      if (gate(t.files, t)) {
        ev.stopImmediatePropagation();
        ev.preventDefault();
      }
    },
    true
  );

  document.addEventListener(
    "drop",
    function (ev) {
      const dt = ev.dataTransfer;
      if (!dt || !dt.files || !dt.files.length) return;
      if (gate(dt.files, null)) {
        ev.stopImmediatePropagation();
        ev.preventDefault();
      }
    },
    true
  );
})();
