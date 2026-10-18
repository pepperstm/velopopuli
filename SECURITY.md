# Security

Please don't report security problems in public issues. Use GitHub's **private vulnerability reporting** (the repository's *Security* tab → *Report a vulnerability*). You'll get a reply as soon as the maintainer can, and credit in the fix if you'd like it.

VeloPopuli stores account tokens (Strava, Home Assistant) in the system keychain, or in files only your user can read on Linux. Diagnostics reports leave out tokens, names and locations; if you find one that doesn't, that's a security issue too.
