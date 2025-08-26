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


