(function () {
  var THEME_KEY = "gist-theme";
  var FONT_KEY = "gist-font";
  var theme = null;
  var font = null;
  try {
    theme = localStorage.getItem(THEME_KEY);
    font = localStorage.getItem(FONT_KEY);
  } catch (e) {}
  if (theme !== "light" && theme !== "dark") theme = "auto";
  if (font !== "vie") font = "eng";
  apply(theme, font);

  function apply(t, f) {
    theme = t;
    font = f;
    var root = document.documentElement;
    root.setAttribute("data-theme", t);
    root.setAttribute("data-font", f);
    root.lang = f === "vie" ? "vi" : "en";
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
    if (sel) {
      sel.value = theme;
      sel.addEventListener("change", function () {
        var t = sel.value;
        if (t !== "light" && t !== "dark") t = "auto";
        try {
          localStorage.setItem(THEME_KEY, t);
        } catch (e) {}
        apply(t, font);
      });
    }

    var fontSel = document.getElementById("font-face");
    if (!fontSel) return;
    fontSel.value = font;
    fontSel.addEventListener("change", function () {
      var f = fontSel.value === "vie" ? "vie" : "eng";
      try {
        localStorage.setItem(FONT_KEY, f);
      } catch (e) {}
      apply(theme, f);
    });
  });
})();
