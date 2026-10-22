# Roadmap

apcast started from sbcast: screen capture, menu and session are shared,
the receiver protocol is new.

## Receiver protocol

Done:

- [x] Find receivers with mDNS (name, id, model)
- [x] Stub sender: runs the capture and drops the frames, so the menu's stats work without a receiver

To do, in order:

- [ ] Find a test receiver
- [ ] Read the receiver's capabilities (`GET /info`) and log them
- [ ] PIN pairing: SRP-6a with a PIN shown on the receiver, then an Ed25519 / X25519 verify step; keep the keys per receiver
- [ ] Access modes an Apple TV can be set to, by hand or by device management (AirPlay Security payload): first-time passcode, passcode each time, or a known password shared by all the devices (the school's case, to confirm). Find how each one shows in `/info` or the mDNS record, and which step takes the password (hypothesis: the pairing step, in place of the PIN). Ask for it in the menu and with `--password`
- [ ] "Same Wi-Fi network only" access: check that it does not block a wired PC
- [ ] Check whether the keys from the verify step are enough to encrypt the streams, before any work on the legacy key exchange
- [ ] Legacy key exchange, only if the check above fails. Main risk: no public specification
- [ ] Session setup: control, timing and data ports
- [ ] Video: H.264 frames, encrypted, over TCP
- [ ] Clock sync for audio and video
- [ ] Sound: AAC-ELD or ALAC over RTP
- [ ] Reconnect after a loss; report reconnections and network drops to the menu
- [ ] PIN entry in the menu and on the command line
- [ ] Test on a real receiver

## Menu

To do:

- [ ] Tune the alert thresholds on a real receiver and the school network

## Packaging

To do:

- [ ] First release: .deb for KDE neon and Ubuntu 26.04
- [ ] Cast for one hour without a cut
