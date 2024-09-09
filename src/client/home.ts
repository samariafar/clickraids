import { games, gameName } from './games';

const escapeHtml = (s: string): string =>
  s.replace(/[&<>"']/g, (c) => ({
    '&': '&amp;',
    '<': '&lt;',
    '>': '&gt;',
    '"': '&quot;',
    "'": '&#39;'
  }[c]!));

export function renderHome(root: HTMLElement, navigate: (slug: string) => void): void {
  document.title = 'ClickRaids — Pick Your Battle';

  const cards = games
    .map((game) => `
      <a class="home__card" href="/${game.slug}">
        <span class="home__card-name">${gameName(game)}</span>
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
}
