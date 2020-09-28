# Debugging notes

One thing that took a while to figure out is that this code would
just hang for a while, from reading the code I became pretty sure
it is the github ratelimit handling in this client library:
https://gitlab.com/crates.rs/crates.rs/-/blob/a00e96d43cb8093dc26d8223a430d6b2cc2556dd/github_v3/src/lib.rs#L191
But I wanted to try verifiying that (and debugging in general) without temporarily forking
that crate to add print statements, and somewhat successfully
used https://github.com/mozilla/rr for debugging.  Being able
to reliably "reverse breakpoint" was *awesome* for something like this that relies
on network requests.  However it became obvious to me that
`gdb` is much less useful when rust async is involved since
stack traces don't give much.  Ended up learning a bit about
https://github.com/tokio-rs/tracing