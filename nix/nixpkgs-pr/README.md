# nixpkgs-pr — first-submission scratch dir

Everything you need to drop into a `nixos/nixpkgs` checkout to open the
first `uncharles` submission (ADR 0004, issue
[leopepe/automata-atelier#52](https://github.com/leopepe/automata-atelier/issues/52)).

This directory is **not** consumed by any in-repo tooling — it is a
copy-paste source. CI does not build from here; the in-repo derivation at
`nix/package.nix` is the one `ci.yml`'s `nix-build` job validates against
the workspace source. Once the nixpkgs PR merges, this directory should be
deleted in a follow-up PR (or kept as a "what we submitted" snapshot if you
prefer).

## What's here

| File | Goes to | Notes |
| --- | --- | --- |
| `package.nix` | `nixpkgs/pkgs/by-name/un/uncharles/package.nix` | Nixpkgs-canonical form. Differences from the in-repo `nix/package.nix`: `src` is no longer parameterised (defaults to `fetchFromGitHub` pinned to the `uncharles-v0.1.0` tag), the dev-only `cargoLockFile` parameter is gone, and `meta.maintainers` references `lib.maintainers.leopepe`. The source `hash` is still `lib.fakeHash` — you replace it in step 3 below. |
| `Cargo.lock` | `nixpkgs/pkgs/by-name/un/uncharles/Cargo.lock` | Verbatim copy of the workspace lockfile at the `uncharles-v0.1.0` tag commit (`c8a8e259`). Co-locating it next to `package.nix` lets `cargoLock.lockFile = ./Cargo.lock;` resolve cleanly without overriding the parameter. |
| `maintainer-list-entry.nix` | Insert into `nixpkgs/maintainers/maintainer-list.nix` (alphabetical) | Keep `nixpkgs-fmt` style. `githubId` is **446756** (verified via `gh api users/leopepe`). |
| `PR_BODY.md` | `gh pr create --body-file …` | Nixpkgs PR template, pre-filled. Tick the platforms you actually tested locally. |

## Step-by-step

```sh
# 0. Prerequisite — install Nix locally if you haven't yet.
#    Determinate Systems installer (cleanest macOS uninstall):
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install

# 1. Fork & clone nixpkgs (one-time).
gh repo fork nixos/nixpkgs --clone --remote
cd nixpkgs
git checkout -b uncharles-init

# 2. Drop files into the right places.
mkdir -p pkgs/by-name/un/uncharles
cp /Users/pepe/Workspace/rust/automata-atelier/nix/nixpkgs-pr/package.nix  pkgs/by-name/un/uncharles/package.nix
cp /Users/pepe/Workspace/rust/automata-atelier/nix/nixpkgs-pr/Cargo.lock   pkgs/by-name/un/uncharles/Cargo.lock

# Then open maintainers/maintainer-list.nix in your editor and paste the
# block from `maintainer-list-entry.nix` in alphabetical order. Run
# `nixpkgs-fmt` on the file afterwards — see step 5.

# 3. Resolve the real source hash, replace lib.fakeHash in package.nix.
nix-shell -p nix-prefetch-github --run \
  'nix-prefetch-github leopepe automata-atelier --rev uncharles-v0.1.0'
# → copy the printed `sha256-...` value into package.nix, replacing `lib.fakeHash`.

# 4. Build & smoke-test.
nix-build -A uncharles
./result/bin/uncharles --help
./result/bin/uncharles --version

# 5. Lint per nixpkgs house style.
nix-shell -p nixpkgs-fmt statix deadnix --run '
  nixpkgs-fmt pkgs/by-name/un/uncharles/ maintainers/maintainer-list.nix
  statix check  pkgs/by-name/un/uncharles/
  deadnix       pkgs/by-name/un/uncharles/
'
# Also run the maintainer-list sort check:
nix-instantiate --eval --strict -E '(import ./maintainers/maintainer-list.nix); true' >/dev/null

# 6. Commit. Two commits, in order; nixpkgs convention.
git add maintainers/maintainer-list.nix
git commit -m "maintainers: add leopepe"

git add pkgs/by-name/un/uncharles
git commit -m "uncharles: init at 0.1.0"

# 7. Push fork + open PR against nixos/nixpkgs:master.
git push -u origin uncharles-init
gh pr create --repo nixos/nixpkgs --base master \
  --title "uncharles: init at 0.1.0" \
  --body-file /Users/pepe/Workspace/rust/automata-atelier/nix/nixpkgs-pr/PR_BODY.md
```

## After the PR is open

1. **Update the GitHub Release** for `uncharles-v0.1.0` upstream to include the nixpkgs PR URL (ADR 0004 confirmation gate #2).
2. **Tick the platforms you actually built locally** in the PR template — leave the others for ofborg to test.
3. **Watch ofborg's comment** (~30 min after opening). It posts a build matrix across `x86_64-linux`, `aarch64-linux`, `x86_64-darwin`, `aarch64-darwin`. Failures arrive as PR comments with logs; fix and push to the same branch.
4. **Respond to reviewer requests** — common feedback: trim `meta` fields, tighten `platforms`, justify the maintainer addition. Push fixes to the same branch.
5. **After merge**, wait for `cache.nixos.org` to build (hours), then smoke-test on a fresh machine:
   `nix-shell -p uncharles --run 'uncharles --help'`. Confirm presence on https://search.nixos.org. Both verifications close ADR 0004's confirmation checklist.
6. **Refresh `uncharles/README.md`** upstream — flip the "Install via Nix" section from placeholder to the real install command. Close [leopepe/automata-atelier#52](https://github.com/leopepe/automata-atelier/issues/52) at the same time.

## Why not just `cp -r nix/package.nix`?

Because the in-repo mirror is parameterised on `src` and `cargoLockFile` so
`nix/ci.nix` can override them to build from the working tree; nixpkgs
wants neither override. `package.nix` here is the de-parameterised form
the nixpkgs reviewer will see, with `meta.maintainers` wired up. Keeping
the two files visibly distinct is easier than `sed`-ing them apart by
hand.

If you change the upstream derivation later, the workflow is:

1. Update `nix/package.nix` upstream (the mirror; CI tests it on every PR).
2. After the upstream PR merges, regenerate `nix/nixpkgs-pr/package.nix`
   from it (drop the dev parameters, leave `meta.maintainers` intact).
3. Open a nixpkgs PR with the regenerated file. For minor version bumps
   the `nixpkgs-update` bot handles this; manual updates are only needed
   for derivation-shape changes.
