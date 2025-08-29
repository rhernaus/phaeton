// Helpers and constants
window.$ = function (id) { return document.getElementById(id); };

window.setTextIfExists = function (id, text) {
  const el = $(id);
  if (el) { el.textContent = text; }
};

window.statusNames = {
  0: 'Disconnected',
  1: 'Connected',
  2: 'Charging',
  3: 'Charged',
  4: 'Wait sun',
  6: 'Wait start',
  7: 'Low SOC',
};

window.getJSON = async function (url) {
  const res = await fetch(url);
  return await res.json();
};

window.postJSON = async function (url, payload, method = 'POST') {
  const res = await fetch(url, {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(payload),
  });
  return await res.json();
};

window.addButtonFeedback = function (button) {
  button.addEventListener('click', function () {
    this.style.transform = 'scale(0.95)';
    setTimeout(() => { this.style.transform = ''; }, 150);
  });
};

// Small helper to format ms to human-friendly age
window.formatAge = function (ms) {
  if (typeof ms !== 'number' || ms < 0) return '-';
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
};

// Parse ANSI color codes to HTML spans with classes
window.ansiToHtml = function (text) {
  if (!text || typeof text !== 'string') return '';

  function escapeHtml(s) {
    return s
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }

  // Regex for ESC[...m sequences
  const sgrRe = /\u001b\[((?:\d{1,3};)*\d{1,3})m/g;
  let out = '';
  let last = 0;
  let openClass = null;

  function closeSpan() {
    if (openClass) { out += '</span>'; openClass = null; }
  }

  function classForCodes(codes) {
    let cls = openClass; // default to current, override if color/reset present
    for (let i = 0; i < codes.length; i++) {
      const c = parseInt(codes[i], 10);
      switch (c) {
        case 0: // reset all
        case 39: // default foreground
          cls = null; break;
        case 30: cls = 'ansi-black'; break;
        case 31: cls = 'ansi-red'; break;
        case 32: cls = 'ansi-green'; break;
        case 33: cls = 'ansi-yellow'; break;
        case 34: cls = 'ansi-blue'; break;
        case 35: cls = 'ansi-magenta'; break;
        case 36: cls = 'ansi-cyan'; break;
        case 37: cls = 'ansi-white'; break;
        case 90: cls = 'ansi-bright-black'; break;
        case 91: cls = 'ansi-bright-red'; break;
        case 92: cls = 'ansi-bright-green'; break;
        case 93: cls = 'ansi-bright-yellow'; break;
        case 94: cls = 'ansi-bright-blue'; break;
        case 95: cls = 'ansi-bright-magenta'; break;
        case 96: cls = 'ansi-bright-cyan'; break;
        case 97: cls = 'ansi-bright-white'; break;
        default: /* ignore other SGR codes like bold, etc. */ break;
      }
    }
    return cls;
  }

  let m;
  while ((m = sgrRe.exec(text)) !== null) {
    // Plain chunk before this SGR -> escape and append
    if (m.index > last) {
      out += escapeHtml(text.slice(last, m.index));
    }

    // Determine the new class to apply
    const codes = m[1].split(';');
    const nextClass = classForCodes(codes);
    if (nextClass !== openClass) {
      // Close previous span if any, then open a new one if needed
      closeSpan();
      if (nextClass) { out += `<span class="${nextClass}">`; openClass = nextClass; }
    }

    last = sgrRe.lastIndex;
  }

  // Trailing plain text
  if (last < text.length) {
    out += escapeHtml(text.slice(last));
  }

  // Ensure we always end with balanced tags
  closeSpan();
  return out;
};


