(function () {
  var KEY = "gist-theme";
  var theme = null;
  try {
    theme = localStorage.getItem(KEY);
  } catch (e) {}
  if (theme !== "light" && theme !== "dark") theme = "auto";
  apply(theme);

  function apply(t) {
    theme = t;
    document.documentElement.setAttribute("data-theme", t);
  }

  function onReady(fn) {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", fn);
    } else {
      fn();
    }
  }

  onReady(function () {
    var sel = document.getElementById("color-scheme");
    if (!sel) return;
    sel.value = theme;
    sel.addEventListener("change", function () {
      var t = sel.value;
      if (t !== "light" && t !== "dark") t = "auto";
      try {
        localStorage.setItem(KEY, t);
      } catch (e) {}
      apply(t);
    });
  });
})();
