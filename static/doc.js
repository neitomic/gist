(function () {
  var KEY = "gist-code-theme";
  var theme = null;
  try {
    theme = localStorage.getItem(KEY);
  } catch (e) {}
  if (!theme) theme = "auto";
  document.documentElement.setAttribute("data-code-theme", theme);

  var TOC_KEY = "gist-toc";
  var tocOn = true;
  try {
    tocOn = localStorage.getItem(TOC_KEY) !== "off";
  } catch (e) {}
  document.documentElement.setAttribute("data-toc", tocOn ? "on" : "off");

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
    wireTocToggle();
    addCopyButtons();
    watchToc();
    loadMermaid();
  });

  var expandedMermaid = null;

  function wireTocToggle() {
    var box = document.getElementById("toc-toggle");
    if (!box) return;
    box.checked = tocOn;
    box.addEventListener("change", function () {
      tocOn = box.checked;
      try {
        localStorage.setItem(TOC_KEY, tocOn ? "on" : "off");
      } catch (e) {}
      document.documentElement.setAttribute("data-toc", tocOn ? "on" : "off");
    });
  }

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
      var codeTheme = document.documentElement.getAttribute("data-code-theme");
      var pageTheme = document.documentElement.getAttribute("data-theme");
      var pageDark =
        pageTheme === "dark" ||
        (pageTheme !== "light" &&
          window.matchMedia &&
          window.matchMedia("(prefers-color-scheme: dark)").matches);
      var dark =
        codeTheme === "ocean-dark" ||
        codeTheme === "mocha" ||
        codeTheme === "eighties" ||
        codeTheme === "solarized-dark" ||
        (codeTheme === "auto" && pageDark);
      window.mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: dark ? "dark" : "default",
      });
      var done = window.mermaid.run({ querySelector: "pre.mermaid" });
      if (done && typeof done.then === "function") {
        done.then(wireMermaidZoom).catch(wireMermaidZoom);
      } else {
        wireMermaidZoom();
      }
    };
    document.head.appendChild(s);
  }

  function wireMermaidZoom() {
    document.querySelectorAll("article.doc .mermaid-wrap").forEach(function (wrap) {
      if (wrap.querySelector("svg")) enhanceMermaid(wrap);
      else watchMermaidSvg(wrap);
    });
  }

  function watchMermaidSvg(wrap) {
    var obs = new MutationObserver(function () {
      if (wrap.querySelector("svg")) {
        obs.disconnect();
        enhanceMermaid(wrap);
      }
    });
    obs.observe(wrap, { childList: true, subtree: true });
  }

  function enhanceMermaid(wrap) {
    if (!wrap || wrap.classList.contains("is-zoomable")) return;
    var svg = wrap.querySelector("svg");
    if (!svg) return;
    wrap.classList.add("is-zoomable");

    var toolbar = document.createElement("div");
    toolbar.className = "mermaid-toolbar";
    toolbar.setAttribute("role", "group");
    toolbar.setAttribute("aria-label", "Diagram zoom");

    var btnOut = mermaidBtn("−", "Zoom out", "out");
    var label = document.createElement("span");
    label.className = "mermaid-zoom-label";
    label.setAttribute("aria-live", "polite");
    label.textContent = "100%";
    var btnIn = mermaidBtn("+", "Zoom in", "in");
    var btnReset = mermaidBtn("Reset", "Reset zoom", "reset");
    var btnExpand = mermaidBtn("Expand", "Expand diagram", "expand");
    btnExpand.setAttribute("aria-expanded", "false");
    toolbar.appendChild(btnOut);
    toolbar.appendChild(label);
    toolbar.appendChild(btnIn);
    toolbar.appendChild(btnReset);
    toolbar.appendChild(btnExpand);

    var viewport = document.createElement("div");
    viewport.className = "mermaid-viewport";
    var stage = document.createElement("div");
    stage.className = "mermaid-stage";
    while (wrap.firstChild) wrap.removeChild(wrap.firstChild);
    stage.appendChild(svg);
    viewport.appendChild(stage);
    wrap.appendChild(toolbar);
    wrap.appendChild(viewport);

    svg.style.maxWidth = "none";
    svg.style.maxHeight = "none";
    var nat = naturalSize(svg);
    svg.setAttribute("width", String(nat.w));
    svg.setAttribute("height", String(nat.h));
    svg.style.width = nat.w + "px";
    svg.style.height = nat.h + "px";
    svg.style.transformOrigin = "0 0";

    var state = {
      wrap: wrap,
      svg: svg,
      viewport: viewport,
      stage: stage,
      label: label,
      expandBtn: btnExpand,
      nat: nat,
      fit: 1,
      scale: 1,
      expanded: false,
      marker: null,
      prevScale: 1,
      prevScroll: [0, 0],
      pointers: {},
      pan: null,
      pinch: null,
    };
    state.fit = computeFit(state);
    state.scale = state.fit;
    applyMermaidLayout(state);
    requestAnimationFrame(function () {
      if (state.expanded) return;
      state.fit = computeFit(state);
      state.scale = state.fit;
      applyMermaidLayout(state);
    });

    btnOut.addEventListener("click", function () {
      zoomMermaid(state, 1 / 1.25);
    });
    btnIn.addEventListener("click", function () {
      zoomMermaid(state, 1.25);
    });
    btnReset.addEventListener("click", function () {
      resetMermaid(state);
    });
    btnExpand.addEventListener("click", function () {
      if (state.expanded) collapseMermaid(state);
      else expandMermaid(state);
    });

    viewport.addEventListener(
      "wheel",
      function (e) {
        if (!(e.ctrlKey || e.metaKey)) return;
        e.preventDefault();
        var factor =
          e.deltaMode === 0 ? Math.exp(-e.deltaY * 0.01) : e.deltaY < 0 ? 1.2 : 1 / 1.2;
        zoomMermaidAt(state, factor, e.clientX, e.clientY);
      },
      { passive: false }
    );

    viewport.addEventListener("pointerdown", function (e) {
      if (e.button !== 0 && e.pointerType === "mouse") return;
      state.pointers[e.pointerId] = { x: e.clientX, y: e.clientY };
      try {
        viewport.setPointerCapture(e.pointerId);
      } catch (err) {}
      mermaidGestureStart(state);
    });
    viewport.addEventListener("pointermove", function (e) {
      if (!state.pointers[e.pointerId]) return;
      state.pointers[e.pointerId] = { x: e.clientX, y: e.clientY };
      mermaidGestureMove(state);
    });
    function pointerEnd(e) {
      delete state.pointers[e.pointerId];
      mermaidGestureEnd(state);
    }
    viewport.addEventListener("pointerup", pointerEnd);
    viewport.addEventListener("pointercancel", pointerEnd);

    state.onKey = function (e) {
      if (e.key === "Escape") {
        e.preventDefault();
        collapseMermaid(state);
      } else if (e.key === "+" || e.key === "=") {
        e.preventDefault();
        zoomMermaid(state, 1.25);
      } else if (e.key === "-" || e.key === "_") {
        e.preventDefault();
        zoomMermaid(state, 1 / 1.25);
      } else if (e.key === "0") {
        e.preventDefault();
        resetMermaid(state);
      }
    };

    var resizeTimer = null;
    window.addEventListener("resize", function () {
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(function () {
        var oldFit = state.fit;
        state.fit = computeFit(state);
        if (Math.abs(state.scale - oldFit) < 0.02) state.scale = state.fit;
        applyMermaidLayout(state);
      }, 120);
    });
  }

  function mermaidBtn(text, aria, act) {
    var b = document.createElement("button");
    b.type = "button";
    b.className = "mermaid-btn";
    b.textContent = text;
    b.setAttribute("aria-label", aria);
    b.title = aria;
    b.setAttribute("data-act", act);
    return b;
  }

  function naturalSize(svg) {
    var w = 0;
    var h = 0;
    var vb = svg.viewBox;
    if (vb && vb.baseVal && vb.baseVal.width && vb.baseVal.height) {
      w = vb.baseVal.width;
      h = vb.baseVal.height;
    }
    if (!w || !h) {
      var aw = parseFloat(svg.getAttribute("width"));
      var ah = parseFloat(svg.getAttribute("height"));
      var widthAttr = svg.getAttribute("width") || "";
      if (aw && ah && widthAttr.indexOf("%") === -1) {
        w = w || aw;
        h = h || ah;
      }
    }
    if (!w || !h) {
      try {
        var box = svg.getBBox();
        w = w || box.width;
        h = h || box.height;
      } catch (e) {}
    }
    if (!w || !h) {
      w = w || svg.clientWidth || 800;
      h = h || svg.clientHeight || 400;
    }
    return { w: Math.max(w, 1), h: Math.max(h, 1) };
  }

  function computeFit(state) {
    var pad = 8;
    var availW = Math.max(40, state.viewport.clientWidth - pad);
    var s = availW / state.nat.w;
    if (state.expanded) {
      var availH = Math.max(40, state.viewport.clientHeight - pad);
      s = Math.min(s, availH / state.nat.h);
    }
    if (s > 1) s = 1;
    if (!(s > 0) || !isFinite(s)) s = 1;
    return s;
  }

  function clampScale(state, s) {
    var min = state.fit * 0.5;
    var max = Math.max(6, state.fit * 8);
    if (s < min) return min;
    if (s > max) return max;
    return s;
  }

  function applyMermaidLayout(state) {
    var w = state.nat.w * state.scale;
    var h = state.nat.h * state.scale;
    state.stage.style.width = w + "px";
    state.stage.style.height = h + "px";
    state.svg.style.transform = "scale(" + state.scale + ")";
    var pct = Math.round((state.scale / state.fit) * 100);
    if (!isFinite(pct) || pct < 1) pct = 100;
    state.label.textContent = pct + "%";
  }

  function zoomMermaid(state, factor) {
    var r = state.viewport.getBoundingClientRect();
    zoomMermaidAt(state, factor, r.left + r.width / 2, r.top + r.height / 2);
  }

  function zoomMermaidAt(state, factor, clientX, clientY) {
    var old = state.scale;
    var next = clampScale(state, old * factor);
    if (Math.abs(next - old) < 0.0001) return;
    var rect = state.viewport.getBoundingClientRect();
    var x = clientX - rect.left;
    var y = clientY - rect.top;
    var contentX = (state.viewport.scrollLeft + x) / old;
    var contentY = (state.viewport.scrollTop + y) / old;
    state.scale = next;
    applyMermaidLayout(state);
    state.viewport.scrollLeft = contentX * next - x;
    state.viewport.scrollTop = contentY * next - y;
  }

  function resetMermaid(state) {
    state.fit = computeFit(state);
    state.scale = state.fit;
    applyMermaidLayout(state);
    state.viewport.scrollLeft = 0;
    state.viewport.scrollTop = 0;
  }

  function mermaidPointers(state) {
    var ids = Object.keys(state.pointers);
    var out = [];
    for (var i = 0; i < ids.length; i++) out.push(state.pointers[ids[i]]);
    return out;
  }

  function mermaidGestureStart(state) {
    var pts = mermaidPointers(state);
    if (pts.length >= 2) {
      state.pan = null;
      var dx = pts[0].x - pts[1].x;
      var dy = pts[0].y - pts[1].y;
      state.pinch = { dist: Math.sqrt(dx * dx + dy * dy) || 1 };
    } else if (pts.length === 1) {
      state.pinch = null;
      state.pan = { x: pts[0].x, y: pts[0].y };
    }
  }

  function mermaidGestureMove(state) {
    var pts = mermaidPointers(state);
    if (pts.length >= 2) {
      var dx = pts[0].x - pts[1].x;
      var dy = pts[0].y - pts[1].y;
      var dist = Math.sqrt(dx * dx + dy * dy) || 1;
      var midX = (pts[0].x + pts[1].x) / 2;
      var midY = (pts[0].y + pts[1].y) / 2;
      if (state.pinch) zoomMermaidAt(state, dist / state.pinch.dist, midX, midY);
      state.pinch = { dist: dist };
      state.pan = null;
      state.viewport.classList.add("is-panning");
    } else if (state.pan && pts.length === 1) {
      state.viewport.scrollLeft -= pts[0].x - state.pan.x;
      state.viewport.scrollTop -= pts[0].y - state.pan.y;
      state.pan = { x: pts[0].x, y: pts[0].y };
      state.viewport.classList.add("is-panning");
    }
  }

  function mermaidGestureEnd(state) {
    var pts = mermaidPointers(state);
    if (pts.length >= 2) {
      var dx = pts[0].x - pts[1].x;
      var dy = pts[0].y - pts[1].y;
      state.pinch = { dist: Math.sqrt(dx * dx + dy * dy) || 1 };
      state.pan = null;
    } else if (pts.length === 1) {
      state.pinch = null;
      state.pan = { x: pts[0].x, y: pts[0].y };
    } else {
      state.pinch = null;
      state.pan = null;
      state.viewport.classList.remove("is-panning");
    }
  }

  function expandMermaid(state) {
    if (state.expanded) return;
    var home = state.wrap.parentNode;
    if (!home) return;
    if (expandedMermaid && expandedMermaid !== state) collapseMermaid(expandedMermaid);
    state.marker = document.createComment("mermaid");
    home.insertBefore(state.marker, state.wrap);
    state.prevScale = state.scale;
    state.prevScroll = [state.viewport.scrollLeft, state.viewport.scrollTop];
    var back = document.createElement("div");
    back.className = "mermaid-backdrop";
    back.addEventListener("click", function () {
      collapseMermaid(state);
    });
    document.body.appendChild(back);
    document.body.appendChild(state.wrap);
    document.body.classList.add("has-mermaid-overlay");
    state.wrap.classList.add("is-expanded");
    state.expanded = true;
    state.backdrop = back;
    expandedMermaid = state;
    state.expandBtn.textContent = "Close";
    state.expandBtn.setAttribute("aria-label", "Close expanded diagram");
    state.expandBtn.title = "Close expanded diagram";
    state.expandBtn.setAttribute("aria-expanded", "true");
    document.addEventListener("keydown", state.onKey);
    state.fit = computeFit(state);
    state.scale = state.fit;
    applyMermaidLayout(state);
    requestAnimationFrame(function () {
      if (!state.expanded) return;
      state.fit = computeFit(state);
      state.scale = state.fit;
      applyMermaidLayout(state);
    });
  }

  function collapseMermaid(state) {
    if (!state.expanded) return;
    document.removeEventListener("keydown", state.onKey);
    if (state.backdrop && state.backdrop.parentNode) {
      state.backdrop.parentNode.removeChild(state.backdrop);
    }
    state.backdrop = null;
    state.wrap.classList.remove("is-expanded");
    if (state.marker && state.marker.parentNode) {
      state.marker.parentNode.insertBefore(state.wrap, state.marker);
      state.marker.parentNode.removeChild(state.marker);
    }
    state.marker = null;
    state.expanded = false;
    if (expandedMermaid === state) expandedMermaid = null;
    document.body.classList.remove("has-mermaid-overlay");
    state.expandBtn.textContent = "Expand";
    state.expandBtn.setAttribute("aria-label", "Expand diagram");
    state.expandBtn.title = "Expand diagram";
    state.expandBtn.setAttribute("aria-expanded", "false");
    state.fit = computeFit(state);
    state.scale = state.prevScale;
    applyMermaidLayout(state);
    state.viewport.scrollLeft = state.prevScroll[0];
    state.viewport.scrollTop = state.prevScroll[1];
  }
})();
