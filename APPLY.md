# Historical vertical-slice overlay instructions

The original overlay has been incorporated into
`1-initial-compiler-implementation`. Use that branch directly for current work;
the compiler's supported scope and local checks are in `compiler/README.md`.
The instructions below describe applying the original archive, not updating the
current implementation.

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

Current CI builds and tests the compiler. Run the documented local checks before
pushing further changes to the development branch.
