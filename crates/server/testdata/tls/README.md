A self-signed certificate and its key for the tests of `ipps` with a pinned certificate
(`tests/label_print.rs`): a pretend printer serves TLS with them. Made for the tests only
(`openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -subj /CN=NPI-TEST`);
never used anywhere else. Its SHA-256 (DER) is
f5d22034e288e86a2de0f79e3863517aeffb56e8832ffe43bd068c866147a35b.
