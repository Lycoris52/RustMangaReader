(function () {
  const buttons = Array.from(document.querySelectorAll('[data-set-lang]'));
  const translatable = Array.from(document.querySelectorAll('[data-en]'));
  const storageKey = 'rustmangareader-lang';

  function applyText(lang) {
    translatable.forEach((node) => {
      const text = node.getAttribute('data-' + lang);
      if (text) node.textContent = text;
    });
  }

  function setLanguage(lang) {
    const selected = lang === 'ja' ? 'ja' : 'en';
    document.body.dataset.lang = selected;
    document.documentElement.lang = selected;
    applyText(selected);

    buttons.forEach((button) => {
      const isActive = button.dataset.setLang === selected;
      button.classList.toggle('is-active', isActive);
      button.setAttribute('aria-pressed', String(isActive));
    });

    try {
      localStorage.setItem(storageKey, selected);
    } catch (error) {
      // Storage may be disabled. The site still works without it.
    }
  }

  buttons.forEach((button) => {
    button.addEventListener('click', () => setLanguage(button.dataset.setLang));
  });

  let saved = 'en';
  try {
    const stored = localStorage.getItem(storageKey);
    if (stored === 'en' || stored === 'ja') {
      saved = stored;
    } else {
      saved = (navigator.language || '').toLowerCase().startsWith('ja') ? 'ja' : 'en';
    }
  } catch (error) {
    saved = (navigator.language || '').toLowerCase().startsWith('ja') ? 'ja' : 'en';
  }

  setLanguage(saved);
})();
