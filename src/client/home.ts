import { games, gameName } from './games';

const escapeHtml = (s: string): string =>
  s.replace(/[&<>"']/g, (c) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;'
  }[c]!));

const SEG_KEYS = ['a', 'b', 'c', 'd', 'e', 'f', 'g'] as const;

// Seven-segment bars in a 12 × 22 viewBox (T=2, S=1).
// a/d/g are horizontal (top/bottom/middle); b/c right (top/bottom); f/e left (top/bottom).
const SEG_PATHS: Record<(typeof SEG_KEYS)[number], string> = {
  a: 'M 2 1 L 3 0 L 9 0 L 10 1 L 9 2 L 3 2 z',
  b: 'M 11 2 L 12 3 L 12 9 L 11 10 L 10 9 L 10 3 z',
  c: 'M 11 12 L 12 13 L 12 19 L 11 20 L 10 19 L 10 13 z',
  d: 'M 2 21 L 3 20 L 9 20 L 10 21 L 9 22 L 3 22 z',
  e: 'M 1 12 L 2 13 L 2 19 L 1 20 L 0 19 L 0 13 z',
  f: 'M 1 2 L 2 3 L 2 9 L 1 10 L 0 9 L 0 3 z',
  g: 'M 2 11 L 3 10 L 9 10 L 10 11 L 9 12 L 3 12 z'
};

const DIGIT_SLOTS = 9;

function digitSlotHtml(): string {
  const paths = SEG_KEYS.map((k) => `<path class="seg seg-${k}" d="${SEG_PATHS[k]}"/>`).join('');
  return `<svg class="counter__digit" viewBox="0 0 12 22" data-digit="0" aria-hidden="true">${paths}</svg>`;
}

function counterMarkup(): string {
  return `<span class="counter" role="img" aria-label="0">${digitSlotHtml().repeat(DIGIT_SLOTS)}</span>`;
}

export function renderHome(root: HTMLElement, navigate: (slug: string) => void): () => void {
  document.title = 'ClickRaids — Pick Your Battle';

  const cards = games
    .map((game) => `
      <a class="home__card" href="/${game.slug}" data-slug="${game.slug}">
        <span class="home__card-name">${gameName(game)}</span>
        <span class="card-stat" data-slug="${game.slug}" title="online players" aria-label="online players">
          <span class="card-stat__icon" aria-hidden="true">👥</span>
          <span class="card-stat__value">–</span>
        </span>
        <div class="home__card-versus">
          <span class="home__card-side">
            <span class="home__card-emoji">${game.emojis[0]}</span>
            <span class="home__card-label">${escapeHtml(game.labels[0])}</span>
          </span>
          <span class="home__card-vs">vs</span>
          <span class="home__card-side">
            <span class="home__card-emoji">${game.emojis[1]}</span>
            <span class="home__card-label">${escapeHtml(game.labels[1])}</span>
          </span>
        </div>
      </a>
    `)
    .join('');

  root.innerHTML = `
    <div class="home">
      <div class="home__inner">
        <h1 class="home__logo">ClickRaids</h1>
        <p class="home__pitch">
          <span>1 million ways to make a difference.</span>
          <span>Pick your battle.</span>
        </p>
        <p class="stats" aria-live="polite">
          ${counterMarkup()}
          <span class="stats__label">players online</span>
        </p>
        <div class="home__grid">${cards}</div>
        <p class="home__footer">
          By <a href="https://samariafar.dev">Sam Ariafar</a>
          · <a href="https://github.com/samariafar/clickraids">GitHub</a>
        </p>
      </div>
    </div>
  `;

  root.querySelectorAll<HTMLAnchorElement>('.home__card').forEach((card) => {
    card.addEventListener('click', (e) => {
      if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
      e.preventDefault();
      const slug = card.getAttribute('href')!.replace(/^\/+/, '');
      history.pushState({}, '', `/${slug}`);
      navigate(slug);
    });
  });

  const statEls = new Map<string, HTMLElement>();
  root.querySelectorAll<HTMLElement>('.card-stat').forEach((el) => {
    const value = el.querySelector<HTMLElement>('.card-stat__value');
    if (value) statEls.set(el.dataset.slug!, value);
  });
  const counterEl = root.querySelector<HTMLElement>('.counter')!;

  const setCounter = (value: number) => {
    const capped = Math.min(Math.max(0, value), 10 ** DIGIT_SLOTS - 1);
    const padded = String(capped).padStart(DIGIT_SLOTS, '0');
    for (let i = 0; i < DIGIT_SLOTS; i++) {
      counterEl.children[i].setAttribute('data-digit', padded[i]);
    }
    counterEl.setAttribute('aria-label', String(value));
  };

  const wsProtocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
  const ws = new WebSocket(`${wsProtocol}//${window.location.host}/ws/stats`);
  ws.binaryType = 'arraybuffer';

  ws.onmessage = (event: MessageEvent) => {
    if (!(event.data instanceof ArrayBuffer)) return;
    const view = new DataView(event.data);
    const n = Math.min(games.length, view.byteLength >> 2);
    let total = 0;
    for (let i = 0; i < n; i++) {
      const count = view.getUint32(i * 4, false);
      total += count;
      const el = statEls.get(games[i].slug);
      if (el) el.textContent = count.toLocaleString();
    }
    setCounter(total);
  };

  return () => ws.close();
}
