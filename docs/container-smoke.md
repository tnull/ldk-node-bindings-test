# Container launcher compatibility smoke test

This test-only branch uses the corrected production Linux build script to
compile native libraries and exercise the Python wheel consumer on fresh
Docker x86_64/ARM64 and rootless Podman x86_64 runners. It compares the two
outputs of each group with the authenticated public reference candidate.

The workflow is dispatched through the existing `smoke-verify.yml` path in
the throwaway repository. It does not create candidate releases or receipts.
This is byte-compatibility evidence for the corrected container launcher,
not a receipt for the older candidate recipe or proof of enforcing SELinux
behavior on Fedora. Keep this harness out of the production PR.
