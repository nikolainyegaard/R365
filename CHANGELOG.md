# Changelog

All notable changes to this project are documented here. The format follows Keep a Changelog and the project follows Semantic Versioning.

## [Unreleased]

### Added
- Entra ID sign-in in the system browser (authorization code with PKCE, handed back through the ms-appx-web scheme handler R365 registers), refresh token kept in the user keyring
- Workspace feed discovery and resource grid grouped by tenant, with search and a cached last feed
- FreeRDP 3 launch for Cloud PCs and RemoteApps with session status on each card, display mode choices in the context menu, and session logs
- Session auth for FreeRDP's Entra ID prompt, completed automatically through the scheme handler, with a paste fallback
- Settings for display, devices, connection and the FreeRDP binary

[Unreleased]: https://github.com/nikolainyegaard/R365/commits/main
