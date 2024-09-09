export interface Game {
  slug: string;
  emojis: [string, string];
  labels: [string, string];
}

interface Meta {
  sides: Record<string, string>;
}

declare const require: {
  context(path: string, deep: boolean, regex: RegExp): {
    keys(): string[];
    (key: string): Meta;
  };
};

const ctx = require.context('../games', true, /meta\.json$/);

export const games: Game[] = ctx
  .keys()
  .map((key) => {
    const slug = key.split('/').at(-2)!;
    const entries = Object.entries(ctx(key).sides);
    return {
      slug,
      labels: [entries[0][0], entries[1][0]],
      emojis: [entries[0][1], entries[1][1]]
    };
  })
  .sort((a, b) => a.slug.localeCompare(b.slug));

const bySlug = new Map(games.map((g) => [g.slug, g]));

export function gameBySlug(slug: string): Game | undefined {
  return bySlug.get(slug);
}

export function gameName(game: Game): string {
  return game.slug.charAt(0).toUpperCase() + game.slug.slice(1);
}
