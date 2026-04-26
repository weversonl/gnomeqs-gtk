/* ===== i18n ===== */
const translations = {
  en: {
    hero_badge: 'Open Source · AGPL-3.0',
    hero_title_1: 'Share files on Linux,',
    hero_title_2: 'the GNOME way.',
    hero_sub: 'Transfer files and text to nearby devices wirelessly. Native GTK4 interface, zero cloud dependency.',
    btn_download: 'Download',
    btn_releases: 'All releases',
    screenshots_title: 'See it in action',
    ss1_title: 'GnomeQS',
    ss2_title: 'Send files',
    features_title: 'Why GnomeQS?',
    feat1_title: 'mDNS Discovery',
    feat1_desc: 'Automatically finds nearby devices on your network. No pairing, no accounts.',
    feat2_title: 'Files & Text',
    feat2_desc: 'Send any file or plain text snippets to nearby devices instantly.',
    feat3_title: 'GNOME Native',
    feat3_desc: 'Built with GTK4 and libadwaita. Feels at home on your GNOME desktop.',
    feat4_title: 'Multiple Packages',
    feat4_desc: 'Available as Flatpak, AUR, Debian, RPM, and AppImage packages.',
    install_title: 'Installation',
    btn_download: 'Download',
    btn_releases: 'All releases',
    install_releases: 'See all releases on GitHub →',
    stable_url_label: 'Stable URL (always latest):',
    footer_github: 'GitHub',
    footer_issues: 'Issues',
  },
  pt: {
    hero_badge: 'Código Aberto · AGPL-3.0',
    hero_title_1: 'Compartilhe arquivos no Linux,',
    hero_title_2: 'do jeito GNOME.',
    hero_sub: 'Transfira arquivos e texto para dispositivos próximos sem fio. Interface GTK4 nativa, sem dependência de nuvem.',
    btn_download: 'Download',
    btn_releases: 'All releases',
    screenshots_title: 'Veja em ação',
    ss1_title: 'GnomeQS',
    ss2_title: 'Enviar arquivos',
    features_title: 'Por que GnomeQS?',
    feat1_title: 'Descoberta mDNS',
    feat1_desc: 'Encontra automaticamente dispositivos próximos na sua rede. Sem pareamento, sem contas.',
    feat2_title: 'Arquivos e Texto',
    feat2_desc: 'Envie qualquer arquivo ou texto para dispositivos próximos instantaneamente.',
    feat3_title: 'Nativo para GNOME',
    feat3_desc: 'Construído com GTK4 e libadwaita. Integrado ao seu desktop GNOME.',
    feat4_title: 'Múltiplos Pacotes',
    feat4_desc: 'Disponível como Flatpak, AUR, Debian, RPM e AppImage.',
    install_title: 'Instalação',
    btn_download: 'Download',
    btn_releases: 'Todos os lançamentos',
    install_releases: 'Ver todos os lançamentos no GitHub →',
    stable_url_label: 'URL estável (sempre a mais recente):',
    footer_github: 'GitHub',
    footer_issues: 'Problemas',
  },
};

let currentLang = 'en';

function detectLang() {
  const saved = localStorage.getItem('lang');
  if (saved && translations[saved]) return saved;
  const nav = (navigator.language || navigator.userLanguage || '').toLowerCase();
  return nav.startsWith('pt') ? 'pt' : 'en';
}

function setLanguage(lang) {
  if (!translations[lang]) return;
  currentLang = lang;
  localStorage.setItem('lang', lang);
  document.documentElement.lang = lang === 'pt' ? 'pt-BR' : 'en';
  document.getElementById('lang-label').textContent = lang === 'pt' ? 'EN' : 'PT';
  document.querySelectorAll('[data-i18n]').forEach(el => {
    const key = el.dataset.i18n;
    if (translations[lang][key] !== undefined) {
      el.textContent = translations[lang][key];
    }
  });
}

/* ===== Theme ===== */
function detectTheme() {
  const saved = localStorage.getItem('theme');
  if (saved) return saved;
  return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function setTheme(theme) {
  document.documentElement.setAttribute('data-theme', theme);
  localStorage.setItem('theme', theme);
  const metaColor = document.getElementById('meta-theme-color');
  if (metaColor) metaColor.content = theme === 'dark' ? '#0b0b18' : '#f8f8ff';
}

function toggleTheme() {
  const current = document.documentElement.getAttribute('data-theme');
  setTheme(current === 'dark' ? 'light' : 'dark');
}

/* ===== Header scroll ===== */
const header = document.getElementById('header');
window.addEventListener('scroll', () => {
  header.classList.toggle('scrolled', window.scrollY > 20);
}, { passive: true });

/* ===== Scroll animations ===== */
const observer = new IntersectionObserver(entries => {
  entries.forEach(e => {
    if (e.isIntersecting) {
      e.target.classList.add('visible');
      observer.unobserve(e.target);
    }
  });
}, { threshold: 0.12, rootMargin: '0px 0px -40px 0px' });

document.querySelectorAll('.animate').forEach(el => observer.observe(el));

/* ===== Tabs ===== */
document.querySelectorAll('.tab-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const tab = btn.dataset.tab;
    document.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));
    document.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));
    btn.classList.add('active');
    document.getElementById('tab-' + tab).classList.add('active');
  });
});

/* ===== Copy to clipboard ===== */
document.querySelectorAll('.copy-btn').forEach(btn => {
  btn.addEventListener('click', () => {
    const code = btn.closest('.code-block').querySelector('code').textContent.trim();
    navigator.clipboard.writeText(code).then(() => {
      btn.classList.add('copied');
      setTimeout(() => btn.classList.remove('copied'), 1800);
    });
  });
});

/* ===== GitHub Release (dynamic version + URLs) ===== */
const REPO = 'weversonl/gnome-quick-share';
const FALLBACK_VERSION = 'v1.4.0';

async function fetchLatestRelease() {
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`);
    if (!res.ok) return;
    const data = await res.json();
    document.querySelectorAll('#hero-version, #footer-version').forEach(el => {
      el.textContent = data.tag_name;
    });
  } catch (_) {
    // silently keep fallback hardcoded values
  }
}

/* ===== Init ===== */
setTheme(detectTheme());
setLanguage(detectLang());
fetchLatestRelease();

document.getElementById('theme-toggle').addEventListener('click', toggleTheme);
document.getElementById('lang-toggle').addEventListener('click', () => {
  setLanguage(currentLang === 'en' ? 'pt' : 'en');
});
