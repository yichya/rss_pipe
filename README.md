# rss_pipe

A small middleware between RSS feed sources and RSS readers to:

* Save RSS content locally (with [rusqlite](https://github.com/rusqlite/rusqlite)) for further use
* Integrate with other content processing services (working in progress)
* Integrate with push services (currently only [Finb/Bark](https://github.com/Finb/Bark) is supported, more will be added later)
* Integrate with reader apps (currently implemented a subset of Fever API; tested with [ReadKit](https://readkit.app/))

Currently, updates are only triggered by external requests, so it is better to this tool with RSS bots:

* [RSS-to-Telegram-Bot](https://github.com/Rongronggg9/RSS-to-Telegram-Bot)
* [flowerss bot](https://github.com/indes/flowerss-bot)

Todo:

* Redirect handling
* Database migrations
* Groups (and maybe GUI for this)
* Presets (proxy, content processing, ...)
* Try to get rid of massive idna / icu dependencies
* Decompressing body (tried but seems not very useful)
* Complete Fever API implementation (since_id, groups, favicons, ...)
* Feed activity tracking (remove feeds not updated for a long time from Fever API)
