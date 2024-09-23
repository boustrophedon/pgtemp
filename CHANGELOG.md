unreleased
----------
- Make shutdown use SIGINT for fast shutdown, which still actively terminates
  connections but does not leak resources.
  This is a breaking change - using the persist option does not use graceful
  shutdown via SIGTERM anymore.

0.4.0
-----
Make shutdown function consume the db

0.3.0
-----
Add -o options to pgtemp daemon for postgresql server configs

0.2
---
Documentation updates

0.1.0
-----
Initial release
