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
  const ESC = "\u001b[";
  const parts = text.split(ESC);
  if (parts.length === 1) return text;
  let out = parts[0];
  for (let i = 1; i < parts.length; i++) {
    const seg = parts[i];
    const m = seg.match(/^([0-9;]+)m(.*)$/s);
    if (!m) { out += seg; continue; }
    const codes = m[1].split(';').map(x => parseInt(x, 10));
    const rest = m[2];
    let cls = '';
    for (const c of codes) {
      switch (c) {
        case 0: cls = ''; break; // reset
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
        default: break;
      }
    }
    if (cls) out += `<span class="${cls}">`;
    out += rest.replace(/\u001b\[[0-9;]*m/g, '</span>');
    if (cls && !out.endsWith('</span>')) out += '</span>';
  }
  return out;
};


