# Apply the Klyxr vertical slice

This package is an **overlay** for the current `gregkrsak/klyxr` repository.

Recommended workflow:

```bash
git switch prod
git pull
git switch -c feat/compiler-vertical-slice

# Copy the contents of this package into the repo root.

git add .
git commit -m "feat: add first Klyxr compiler vertical slice"
git push -u origin feat/compiler-vertical-slice
```

Then open a pull request into `prod`.

The first CI run is important: it will be the first real Rust compilation of this slice.
