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

  function two(n) {
    return (n < 10 ? "0" : "") + n;
  }

  function sameDay(a, b) {
    return (
      a.getFullYear() === b.getFullYear() &&
      a.getMonth() === b.getMonth() &&
      a.getDate() === b.getDate()
    );
  }

  // The server label is UTC. Rewrite it from the viewer's local clock.
  function localTimeLabel(iso) {
    var t = new Date(iso);
    if (isNaN(t.getTime())) return null;
    var now = new Date();
    var hm = two(t.getHours()) + ":" + two(t.getMinutes());
    if (sameDay(t, now)) return "today " + hm;
    var yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1);
    if (sameDay(t, yesterday)) return "yesterday " + hm;
    var months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    return t.getDate() + " " + months[t.getMonth()] + " " + t.getFullYear();
  }

  function showLocalTimes() {
    Array.prototype.forEach.call(document.querySelectorAll("time[datetime]"), function (el) {
      var label = localTimeLabel(el.getAttribute("datetime"));
      if (label) el.textContent = label;
    });
  }

  onReady(function () {
    showLocalTimes();
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
