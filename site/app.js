// HeroWorld website: cover rays, the noon/night slider, panels that pop
// in, the theme button, and the newest version and changes read live from
// GitHub (the page shows a built-in copy when GitHub cannot be reached).
(function () {
  "use strict";
  var REPO = "ArndRosemeier/HeroWorld";
  var still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ---- Turning sunburst on the cover ----
  var canvas = document.querySelector(".rays");
  if (canvas && canvas.getContext) {
    var ctx = canvas.getContext("2d");
    var draw = function (t) {
      var w = canvas.clientWidth, h = canvas.clientHeight, dpr = Math.min(window.devicePixelRatio || 1, 2);
      if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
        canvas.width = Math.round(w * dpr); canvas.height = Math.round(h * dpr);
      }
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      ctx.clearRect(0, 0, w, h);
      var cx = w * 0.3, cy = h * 0.18, r = Math.hypot(w, h) * 1.2, n = 22;
      var turn = still ? 0 : t * 0.00004;
      for (var i = 0; i < n; i++) {
        var a0 = turn + (i / n) * Math.PI * 2, a1 = a0 + Math.PI / n;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.lineTo(cx + Math.cos(a0) * r, cy + Math.sin(a0) * r);
        ctx.lineTo(cx + Math.cos(a1) * r, cy + Math.sin(a1) * r);
        ctx.closePath();
        ctx.fillStyle = i % 2 ? "rgba(90, 110, 220, 0.35)" : "rgba(255, 198, 64, 0.10)";
        ctx.fill();
      }
      if (!still) requestAnimationFrame(draw);
    };
    requestAnimationFrame(draw);
  }

  // ---- Noon to night ----
  var compare = document.getElementById("compare");
  var slider = document.getElementById("daynight");
  var night = document.getElementById("night");
  if (compare && slider && night) {
    var set = function () {
      var v = Number(slider.value);
      night.style.clipPath = "inset(0 0 0 " + v + "%)";
      compare.style.setProperty("--split", v + "%");
    };
    slider.addEventListener("input", set);
    set();
  }

  // ---- Panels pop as they come into view (they are visible before) ----
  if ("IntersectionObserver" in window && !still) {
    var io = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) {
        if (e.isIntersecting) { e.target.classList.add("seen"); io.unobserve(e.target); }
      });
    }, { threshold: 0.25 });
    document.querySelectorAll(".pop").forEach(function (el) { io.observe(el); });
  }

  // ---- The title theme, only when asked ----
  var btn = document.getElementById("sound");
  var audio = document.getElementById("theme");
  if (btn && audio) {
    btn.addEventListener("click", function () {
      if (audio.paused) {
        audio.volume = 0.6;
        audio.play().then(function () {
          btn.setAttribute("aria-pressed", "true");
          btn.setAttribute("aria-label", "Stop the title theme");
        }).catch(function () {
          btn.querySelector(".sound-text").textContent = "No sound";
        });
      } else {
        audio.pause();
        btn.setAttribute("aria-pressed", "false");
        btn.setAttribute("aria-label", "Play the title theme");
      }
    });
  }

  // ---- Live: the newest build and what changed ----
  function esc(s) {
    return s.replace(/[&<>"]/g, function (c) { return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]; });
  }
  function inline(s) {
    return esc(s).replace(/\*\*(.+?)\*\*/g, "<b>$1</b>").replace(/\*(.+?)\*/g, "<i>$1</i>").replace(/`(.+?)`/g, "$1");
  }

  fetch("https://api.github.com/repos/" + REPO + "/releases/tags/windows-latest")
    .then(function (r) { return r.ok ? r.json() : Promise.reject(r.status); })
    .then(function (rel) {
      var m = /HeroWorld (\d+\.\d+)/.exec(rel.name || rel.body || "");
      if (m) document.getElementById("issue-no").textContent = "No. " + m[1];
      var zip = (rel.assets || []).filter(function (a) { return a.name === "HeroWorld-windows.zip"; })[0];
      if (zip) document.getElementById("size").textContent = Math.round(zip.size / 1e6) + " MB";
      var launcher = (rel.assets || []).some(function (a) { return a.name === "HeroWorldLauncher.exe"; });
      if (!launcher) {
        // Not published yet (or the nightly build is being replaced): offer the zip.
        var dl = document.getElementById("download");
        dl.href = "https://github.com/" + REPO + "/releases/tag/windows-latest";
        document.getElementById("download-sub").textContent = m ? "HeroWorld " + m[1] + ", as a zip" : "as a zip";
      } else if (m) {
        document.getElementById("download-sub").textContent = "HeroWorld " + m[1] + ". The launcher installs it and keeps it up to date";
      }
    })
    .catch(function () { /* the built-in text stays */ });

  fetch("https://raw.githubusercontent.com/" + REPO + "/main/WHATS_NEW.md")
    .then(function (r) { return r.ok ? r.text() : Promise.reject(r.status); })
    .then(function (md) {
      var items = [], cur = null;
      md.split("\n").forEach(function (line) {
        var h = /^## (\S+)\s*(?:\((.*)\))?/.exec(line);
        if (h) { cur = { v: h[1], date: h[2] || "", lines: [] }; items.push(cur); return; }
        if (cur && /^- /.test(line)) cur.lines.push(line.slice(2));
        else if (cur && cur.lines.length && /^\s+\S/.test(line)) cur.lines[cur.lines.length - 1] += " " + line.trim();
      });
      if (!items.length) return;
      var html = items.slice(0, 6).map(function (it) {
        return '<article class="news-item"><h3>' + esc(it.v) + (it.date ? " <small>" + esc(it.date) + "</small>" : "") +
          "</h3><ul>" + it.lines.map(function (l) { return "<li>" + inline(l) + "</li>"; }).join("") + "</ul></article>";
      }).join("");
      document.getElementById("news-list").innerHTML = html;
    })
    .catch(function () { /* the built-in text stays */ });
})();
