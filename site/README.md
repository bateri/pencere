# The pencere site

The site at [pencere.dev](https://pencere.dev): what pencere is, and its documentation.

It says nothing of pencere itself. What changes when the code changes is said in the repository,
beside the code, and the site reads it when it is built: `src/docs.js` is the map of which page
shows which file, and says the rule.

```sh
npm ci
npm run dev        # the site, watched: a change to a file of the repository a page shows is shown too
npm run build      # dist/, after the site and the repository are checked against each other
```

A push to `main` builds and deploys it (Cloudflare's own build, with this directory as its root;
`wrangler.jsonc`), and the `site` workflow builds it on every push and pull request.

The pictures of the elements and the components (`public/shots/`) are made by `shots.sh` from the
page scripts under `demos/`, with the drive's example `shots`.

The site is under the repository's licence, but for the footage of its first scene: [NOTICE](NOTICE).
