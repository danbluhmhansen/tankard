interface Game {
  id: string,
  new: boolean,
  drop: boolean,
  set: boolean,
}

window.Tankard = {
  hello: () => {console.log("Hello, World!");},
  gamesSubmit: async (games: Game[]) => {
    let promises = [];

    if (games.some(g => g.new && !g.drop))
        promises.push(fetch('/api/games', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json', },
            body: JSON.stringify(games.filter(g => g.new && !g.drop))
        }));

    if (games.some(g => g.set))
        promises.push(fetch('/api/games', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json', },
            body: JSON.stringify(games.filter(g => g.set))
        }));

    if (games.some(g => g.drop && !g.new))
        promises.push(fetch(`/api/games?ids=${games.filter(g => g.drop && !g.new).map(g => g.id).join('&ids=')}`, {
            method: 'DELETE',
        }));

    await Promise.all(promises);
  },
};

declare global {
  interface Window {
    Tankard: any;
  }
}
