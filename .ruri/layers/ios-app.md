# Una — iOS app

One layer of Una's stack, owning `client/apps/ios/`. The whole stack, and how the layers connect, is in `.ruri/architecture.md`.
Don't edit this file: ruri writes it. `ruri layer ios-app` prints it with every line numbered, what git says changed in this layer lately and what sessions learned working here; once you have read it that way, put right what your work changed — `ruri layer ios-app add|set|drop <section> …`. Where it and the code disagree, the code is right.

A native iPhone app for Una, written in Swift and SwiftUI for iOS 17+ with no third-party packages, built with DEVELOPER_DIR=/Volumes/T7/Stockpile/Xcode.app/Contents/Developer. It records or loads 16 kHz mono WAV audio, uploads it to a user-chosen una server over HTTP, shows the transcript with copy to clipboard, and lets the user correct a past dictation from History; networking, models, corrections and WAV encoding live in the UnaKit package. A keyboard extension cannot record, so its mic key signals the containing app through an App Group flag and a Darwin notification, and the app records, uploads and returns the transcript; without an App Group (a free Apple Personal Team) the mic key shows a notice and typing still works. The idea to grasp first is that nothing is tied to one user: the server list starts empty, onboarding collects addresses, and the bundle ID and App Group live only in Config/Una.xcconfig.

## Where to change what

- **the UnaKit package:** client/apps/ios/UnaKit/Sources/UnaKit/, client/apps/ios/UnaKit/Package.swift
- **endpoint race (choosing which server address answers, health-check timeouts):** client/apps/ios/UnaKit/Sources/UnaKit/Endpoints.swift, client/apps/ios/UnaKit/Sources/UnaKit/Client.swift, client/apps/ios/UnaKit/Tests/UnaKitTests/UnaKitTests.swift
- **dictation engine (capture to transcript pipeline):** client/apps/ios/UnaKit/Sources/UnaKit/DictationEngine.swift, client/apps/ios/Una/Recorder.swift
- **corrections (edit a dictation's text, word diff, filed status):** client/apps/ios/UnaKit/Sources/UnaKit/Corrections.swift, client/apps/ios/Una/FixView.swift, client/apps/ios/UnaKit/Sources/UnaKit/Models.swift, client/apps/ios/UnaKit/Tests/UnaKitTests/CorrectionTests.swift
- **the keyboard mic key and text insertion (step 2b, space before a pasted transcript):** client/apps/ios/UnaKeyboard/KeyboardViewController.swift, client/apps/ios/UnaKit/Sources/UnaKit/KeyboardLogic.swift, client/apps/ios/UnaKeyboard/KeyboardView.swift, client/apps/ios/UnaKit/Sources/UnaKit/MicRemote.swift
- **mic handoff between keyboard and app (App Group flag, Darwin notifications, local-base fallback):** client/apps/ios/UnaKit/Sources/UnaKit/Handoff.swift, client/apps/ios/UnaKit/Tests/UnaKitTests/HandoffTests.swift
- **the Record screen (capture, upload, result card, DEBUG WAV feed):** client/apps/ios/Una/RecordView.swift, client/apps/ios/Una/AppModel.swift, client/apps/ios/Una/DebugFeed.swift
- **first-run onboarding (server address, Test connection, Tailscale and Full Access text):** client/apps/ios/Una/OnboardingView.swift, client/apps/ios/Una/AppModel.swift
- **the Servers tab and Privacy section:** client/apps/ios/Una/ServersView.swift
- **the History tab (past dictations, opens a dictation to correct it):** client/apps/ios/Una/HistoryView.swift, client/apps/ios/Una/FixView.swift

## How it works

- **Dictate and send:** The Record tab captures mic audio (or the DEBUG WAV feed in debug builds) → UnaKit's WAV encoder converts it to 16 kHz mono WAV → The spool holds the upload in the App Group container, or Application Support when no App Group is signed, and retries on failure → The API client sends it through the endpoint race across the saved server addresses → The transcript returns; the result card shows it and copies it to the clipboard → The History tab lists past dictations
- **Correct a dictation:** History opens FixView with raw and cleaned text → the user edits and saves → Corrections builds the request → AppModel sends it through the endpoint race; if unreachable the CorrectionOutbox (App Group container, or Application Support without one) keeps it and retries after History reloads (4xx drops it) → the screen shows the word diff and filed status
- **Mic key dictation (step 2b):** If no App Group is signed, the keyboard's mic key shows a notice and stops → The keyboard's mic key writes a request flag to the App Group container and posts a Darwin notification → The app, holding an active background audio session, starts recording 16 kHz mono → UnaKit encodes the WAV and uploads it through the endpoint race → The transcript is written to the shared container and a Darwin notification tells the keyboard → The keyboard reads the transcript and inserts it into the host field, with one leading space when the text before the cursor doesn't end in whitespace
- **First run:** UnaApp shows OnboardingView when no server is saved → The user types a server address and taps Test connection → The test runs the endpoint race; a working address is saved to the server list in AppModel → The Servers tab edits the list later

## Key files

- client/apps/ios/UnaKit/Sources/UnaKit/Endpoints.swift — races the saved server addresses and picks the first that answers
- client/apps/ios/UnaKit/Sources/UnaKit/DictationEngine.swift — drives capture, upload and transcript return
- client/apps/ios/UnaKit/Sources/UnaKit/Client.swift — API client that sends uploads and corrections, with health-check timeouts
- client/apps/ios/UnaKit/Sources/UnaKit/Corrections.swift — builds correction requests and the word diff
- client/apps/ios/UnaKit/Sources/UnaKit/Models.swift — shared request and response models
- client/apps/ios/UnaKit/Sources/UnaKit/Handoff.swift — shared-container flags, notifications, and the local-base fallback when no App Group
- client/apps/ios/UnaKit/Sources/UnaKit/MicRemote.swift — keyboard-side remote that drives the app's mic
- client/apps/ios/UnaKit/Sources/UnaKit/KeyboardLogic.swift — decides when a transcript needs a leading space
- client/apps/ios/UnaKeyboard/KeyboardViewController.swift — keyboard host: mic key, Full Access notice, typing
- client/apps/ios/Una/AppModel.swift — shared app state, server list and first-run flag

## Rules and traps

- Build: DEVELOPER_DIR=/Volumes/T7/Stockpile/Xcode.app/Contents/Developer; xcodebuild -scheme Una -destination 'platform=iOS Simulator,name=iPhone 17 Pro' CODE_SIGNING_ALLOWED=NO. DEBUG feed: -UnaDebugWAV <path> -UnaDebugAutorun YES
- Deployment target is iOS 17.0 (app, keyboard, UnaKit); iOS 18+ APIs need `if #available`. Swift 6 mode: build off-main callbacks (the AVAudioEngine tap) in a nonisolated static func, never inline in a @MainActor type, or they trap.
- No personal IDs, hostnames, IPs, paths, names or bundle IDs in code; the bundle ID and App Group belong only in Config/Una.xcconfig.
- DEVELOPMENT_TEAM is passed on the xcodebuild command line and never stored in the project or in Swift.
- A free Personal Team cannot sign com.apple.security.application-groups, so a device build for it must drop the entitlements file and the mic handoff is unavailable.
- Paid-team archives fail with an app-group profile mismatch until the App Group exists in the developer portal and is attached to both the app and keyboard App IDs; the App Store Connect API cannot create it.
- TestFlight export uses manual signing with App Store profiles, because the App Manager API key cannot use Apple cloud signing and automatic export fails (steps in docs/iphone.md).
- Keyboard space insertion adds nothing when the field is empty, after whitespace, or when the transcript already starts with a space.

## What it talks to

- Una server (server/) — the app uploads WAV and sends corrections to the server's HTTP API on port 8100, the same correction API the desktop client and dashboard use; the server binds 0.0.0.0 (server/src/una_server/config.py) so Tailscale traffic reaches it
- Tailscale — the phone reaches the home server over the tailnet using the MagicDNS name or 100.x address the user enters in onboarding
- Keyboard extension — the mic key sends a request through the App Group and a Darwin notification; the app returns the transcript the same way
- Wire contract — the mic handoff keys and notification names are defined in UnaKit's Handoff.swift and shared by both targets
- Apple developer account — a paid team signs the App Group the mic handoff needs and TestFlight uploads go through App Store Connect; a free Personal Team builds and runs everything except the mic handoff
