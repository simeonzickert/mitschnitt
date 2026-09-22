# Why there are no workflows here

Mitschnitt is built, signed and notarized locally with `scripts/mitschnitt-release.sh`.
The CI pipeline of the project it started from expected that project's own cloud
credentials, runners and deploy targets, so it was removed rather than left to fail.
To build from source, see the README.
