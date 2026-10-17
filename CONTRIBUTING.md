# Contributing to Multron Download Manager

Thank you for helping build Multron. Please read this page before you open a pull request.

## Who can contribute

Multron Download Manager is **source-available, proprietary software** (see [LICENSE](LICENSE)). Anyone can read the code, but only **Authorized Developers** may copy, change and build it — people the Multron Community has invited, for example members of the Multron Community organization on GitHub or people with write access to this repository (LICENSE, section 6).

- Bug reports, ideas and translation suggestions are welcome from everyone through GitHub Issues.
- If you would like to contribute code, ask the maintainers to add you to the organization first.

## Your contributions

By opening a pull request, pushing to this repository or sending code, translations or other material, you agree that:

- your contribution is your own work, or you have the right to give it;
- the Multron Community may use, change, publish, relicense and sell it as part of Multron Download Manager or otherwise (LICENSE, section 5);
- you will not publish, share or reuse Multron Download Manager's source code outside the official repositories, and your permission to work on it ends when you leave the organization (LICENSE, section 6).

Do not add code you copied from projects whose license does not allow this, such as GPL code. Code under permissive licenses (MIT, Apache-2.0, BSD, ISC, Zlib) is fine. Add new dependencies to [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## Workflow

1. Create a branch from `main` (`feat/...`, `fix/...`).
2. Make your change and keep it focused on one topic.
3. Run the checks:
   ```bash
   npm test
   npm run check
   ```
   `npm run check` runs clippy, svelte-check and the translation check.
4. Open a pull request against `main`. Do not push directly to `main`.

## Style

- Follow the existing code: Rust is formatted with `cargo fmt`, the UI uses Svelte 5 and Tailwind CSS v4.
- Do not add code comments; name things clearly instead.
- All UI text is written in English and wrapped in `t('…')`. Add the translation for every language file in `frontend/src/lang/` — `npm run check:i18n` lists anything missing.
- The app is English by default and follows the system language.
