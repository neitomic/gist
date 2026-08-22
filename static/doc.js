(function () {
  var KEY = "gist-code-theme";
  var theme = null;
  try {
    theme = localStorage.getItem(KEY);
  } catch (e) {}
  if (!theme) theme = "auto";
  document.documentElement.setAttribute("data-code-theme", theme);

  function onReady(fn) {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", fn);
    } else {
      fn();
    }
  }

  onReady(function () {
    var sel = document.getElementById("code-theme");
    if (sel) {
      sel.value = theme;
      sel.addEventListener("change", function () {
        theme = sel.value || "auto";
        try {
          localStorage.setItem(KEY, theme);
        } catch (e) {}
        document.documentElement.setAttribute("data-code-theme", theme);
      });
    }
    addCopyButtons();
    watchToc();
    loadMermaid();
  });

  function addCopyButtons() {
    document.querySelectorAll("article.doc .code-block").forEach(function (block) {
      if (block.querySelector(".copy-btn")) return;
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "copy-btn";
      btn.textContent = "Copy";
      btn.addEventListener("click", function () {
        var code = block.querySelector("pre");
        var text = code ? code.innerText : "";
        if (navigator.clipboard && navigator.clipboard.writeText) {
          navigator.clipboard.writeText(text).then(
            function () {
              btn.textContent = "Copied";
              setTimeout(function () {
                btn.textContent = "Copy";
              }, 1200);
            },
            function () {}
          );
        }
      });
      var meta = block.querySelector(".code-meta");
      if (meta) meta.appendChild(btn);
      else block.insertBefore(btn, block.firstChild);
    });
  }

  function watchToc() {
    var links = Array.prototype.slice.call(document.querySelectorAll(".toc a[href^='#']"));
    if (!links.length || !("IntersectionObserver" in window)) return;
    var map = {};
    links.forEach(function (a) {
      map[decodeURIComponent(a.getAttribute("href").slice(1))] = a;
    });
    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          var a = map[entry.target.id];
          if (!a) return;
          if (entry.isIntersecting) {
            links.forEach(function (l) {
              l.classList.remove("active");
            });
            a.classList.add("active");
          }
        });
      },
      { rootMargin: "0px 0px -70% 0px", threshold: 0.1 }
    );
    Object.keys(map).forEach(function (id) {
      var el = document.getElementById(id);
      if (el) observer.observe(el);
    });
  }

  function loadMermaid() {
    if (!document.querySelector("pre.mermaid")) return;
    var s = document.createElement("script");
    s.src = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.min.js";
    s.onload = function () {
      if (!window.mermaid) return;
      var dark =
        document.documentElement.getAttribute("data-code-theme") === "ocean-dark" ||
        document.documentElement.getAttribute("data-code-theme") === "mocha" ||
        document.documentElement.getAttribute("data-code-theme") === "eighties" ||
        document.documentElement.getAttribute("data-code-theme") === "solarized-dark" ||
        (document.documentElement.getAttribute("data-code-theme") === "auto" &&
          window.matchMedia &&
          window.matchMedia("(prefers-color-scheme: dark)").matches);
      window.mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: dark ? "dark" : "default",
      });
      window.mermaid.run({ querySelector: "pre.mermaid" });
    };
    document.head.appendChild(s);
  }
})();
