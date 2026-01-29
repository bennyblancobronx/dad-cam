# Dad Cam - Licensing & Business Strategy

Version: 2.0.0
Status: Planning
Phase: 9

---

## The Problem

How do we:
1. Make money from Dad Cam
2. Give it free to rental clients
3. Prevent (reduce) piracy
4. Keep it simple and user-friendly
5. Respect "no cloud dependency" principle

---

## Philosophy

### The Hard Truth About Desktop App Piracy

**Any local-only protection can be cracked.** Period.

- Binary patching bypasses any check
- System clock changes defeat time trials
- License files can be faked
- Machine IDs can be spoofed

**Online activation** (phone home) is the only "real" protection, but:
- Requires server infrastructure
- Fails when offline (your users are often offline - working with old footage)
- Users hate it
- Violates "no cloud dependency" contract

### The Pragmatic Approach

**Don't fight piracy. Make buying easier than pirating.**

| Pirate's Experience | Buyer's Experience |
|---------------------|---------------------|
| Find crack site (risky) | Click "Buy" |
| Download sketchy file | Enter card |
| Disable antivirus | Get key instantly |
| Hope it works | Done |
| No updates | Auto-updates |
| No support | Email support |
| Guilt | Pride of ownership |

**Target market reality:**
- Dad Cam users are parents/grandparents with old footage
- They're not tech-savvy pirates
- $99 is reasonable for preserving family memories
- The 1% who pirate probably wouldn't have paid anyway

### The 90/9/1 Rule

- **90%** will pay if it's easy and fair
- **9%** will pirate no matter what
- **1%** are on the fence (these are who you target with friction)

**Goal:** Make the 90% happy. Add enough friction to convert some of the 1%. Ignore the 9%.

---

## Recommended Model

### Pricing

| Tier | Price | Who | How |
|------|-------|-----|-----|
| Trial | Free | Anyone | 14 days, full features |
| Personal | $99 | Anyone | One-time, lifetime |
| Rental | Free | Your clients | Key included with rental |
| Dev | Free | You | Hardcoded |

**Why one-time, not subscription:**
- Users HATE subscriptions for local apps
- "Pay once, own forever" is a selling point
- $99 is impulse-buy territory for target market
- Simpler accounting, no churn management

### Trial Strategy

**14 days, full features, then soft lock.**

```
Days 1-11:  Full features, subtle "X days left" in footer
Days 12-14: Full features, prominent banner warning
Day 15+:    Soft lock (see below)
```

**Soft Lock (not hard lock):**
- CAN still open app
- CAN still view/browse library
- CAN still play clips
- CANNOT import new footage
- CANNOT export
- CANNOT auto-edit
- Watermark on any screenshots/shares

**Why soft lock:**
- Hard lock feels punitive, creates anger
- Soft lock lets them see what they're missing
- Their library is hostage (nicely) - they want to USE it
- Every time they open app, they see the buy prompt

### License Key System

**Format:** `DCAM-XXXX-XXXX-XXXX-XXXX`

```
DCAM-P-XXXX-XXXX-XXXX  = Purchased
DCAM-R-XXXX-XXXX-XXXX  = Rental (free)
DCAM-D-XXXX-XXXX-XXXX  = Dev
```

**Generation:**
- Purchased keys: Generated at checkout (Gumroad, Paddle, or custom)
- Rental keys: You generate in dev menu, give to client
- Dev key: Hardcoded or env var

**Validation:**
- Local only (no phone home)
- Simple checksum/signature verification
- Not cryptographically secure (accept this)

---

## Anti-Piracy Strategy

### What We Do (Low Friction)

| Protection | Stops | Effort |
|------------|-------|--------|
| Machine ID | Casual reinstall | Low |
| Obfuscated license check | Casual inspection | Low |
| Multiple check locations | Single-point patch | Medium |
| Watermark when unlicensed | Sharing pirated exports | Low |

### What We Don't Do (High Friction, Low ROI)

| Protection | Why Not |
|------------|---------|
| Online activation | Violates offline principle, users hate it |
| Hardware dongle | Overkill for $99 app |
| Aggressive DRM | Creates support burden, angers legit users |
| Legal threats | Expensive, bad PR, doesn't work |

### Machine ID Implementation

```rust
fn generate_machine_id() -> String {
    let mut hasher = blake3::Hasher::new();

    // Inputs that survive reinstall
    hasher.update(hostname().as_bytes());
    hasher.update(username().as_bytes());

    // Hardware-ish identifiers
    #[cfg(target_os = "macos")]
    hasher.update(get_mac_serial().as_bytes());

    #[cfg(target_os = "windows")]
    hasher.update(get_windows_product_id().as_bytes());

    // First 16 chars of hash
    hex::encode(&hasher.finalize().as_bytes()[..8])
}
```

**Stored in:**
1. App data folder (primary)
2. System keychain (backup - survives app uninstall)

**If machine ID changes** (new computer): User contacts support for key transfer. This is rare and manual is fine.

### Obfuscated License Check

Don't make one obvious `check_license()` function. Spread checks throughout:

```rust
// In import.rs
fn start_import(...) {
    if !is_feature_available("import") {  // Innocuous name
        show_upgrade_prompt();
        return;
    }
    // ... import code
}

// In export.rs
fn render_export(...) {
    verify_app_state();  // Another innocuous name
    // ... export code
}

// Different check styles make patching harder
fn is_feature_available(feature: &str) -> bool { ... }
fn verify_app_state() -> bool { ... }
fn get_app_mode() -> AppMode { ... }
```

**This isn't security through obscurity** - it's friction through obscurity. Any determined cracker will find all checks. But casual pirates won't bother.

### Watermark Strategy

When unlicensed/expired:

```
Viewing:     No watermark (let them see their memories)
Export:      "Dad Cam Trial" watermark bottom-right
Screenshot:  Watermark (if we can detect)
Share:       Watermark
```

**Watermark design:**
- Small, semi-transparent, corner position
- Not obnoxious (don't ruin the memory)
- Just enough to discourage sharing pirated exports

---

## Rental Client Strategy

### How Rental Works

1. Client rents camera from you
2. They return camera with footage
3. You import footage, deliver library to them
4. They get Dad Cam free to view/edit their memories

### Key Distribution

**Option A: Pre-generated keys**
```
You generate 100 rental keys in advance
Store in spreadsheet
Give one to each client
Track which client has which key
```

**Option B: On-demand generation**
```
Client needs key
You open dev menu
Generate key tied to their email/name
Give to client
```

**Option C: No key needed**
```
Libraries you create are "blessed"
When client opens library, app sees it's from a licensed source
No separate key needed
```

**Recommendation: Option A** (simplest)
- Pre-generate batch of keys
- Give one per rental
- If they share key, who cares - they're your clients

### Rental Key Limits?

**Don't limit rental keys.** Your rental clients are:
- Already paying you for the rental
- Good word-of-mouth sources
- Not the piracy risk

If a rental client gives their key to a friend, that friend might:
- Become a rental client
- Buy their own license
- Tell others about the app

**This is marketing, not loss.**

---

## Purchase Flow

### Recommended: Gumroad or Paddle

**Why not custom payment:**
- Payment processing is complex (PCI compliance, fraud, chargebacks)
- Gumroad/Paddle handle everything
- They take ~5-10% but save massive headache

**Flow:**
```
User clicks "Buy Now" in app
  --> Opens browser to gumroad.com/l/dadcam
  --> User pays $99
  --> Gumroad shows license key immediately
  --> User copies key, pastes in app
  --> Done
```

**Gumroad benefits:**
- Handles payments globally
- Generates unique keys automatically
- Handles refunds
- Provides sales dashboard
- No server needed on your end

### In-App Purchase Flow

```
[Trial Expired Dialog]

Your 14-day trial has expired.

To keep using Dad Cam:
  - Import new footage
  - Export your memories
  - Create VHS-style films

[Buy Now - $99]  [I Have a Key]

---

[Enter License Key Dialog]

Paste your license key:
[DCAM-________________]

[Activate]  [Buy a Key]
```

---

## Dev Menu

Secret menu for your use only.

### Access

- **Shortcut:** Cmd+Shift+D (Mac) / Ctrl+Shift+D (Win)
- **Or:** Settings > About > Click version 7 times

### Features

```
Dev Menu
│
├── Register Camera        <-- YOUR RENTAL WORKFLOW
│   ├── Via USB
│   │   └── Auto-detect connected camera
│   │   └── Extract USB fingerprint
│   │   └── EXIF dump from sample files
│   │   └── Fill camera profile form
│   └── Via Memory Card
│       └── Scan card for sample files
│       └── EXIF dump
│       └── Manual entry for missing fields
│
├── License Tools
│   ├── View current license state
│   ├── Generate rental key
│   ├── Clear license (test trial)
│   └── Set dev mode
│
├── Camera Database
│   ├── View all cameras (bundled + custom)
│   ├── Add/edit camera manually
│   ├── Import JSON
│   └── Export JSON
│
├── Database
│   ├── SQLite browser
│   ├── Run raw SQL
│   ├── Export database
│   └── Reset database
│
├── Debug
│   ├── View logs
│   ├── FFmpeg test
│   ├── Clear caches
│   └── Crash test
│
└── Info
    ├── Machine ID
    ├── Trial info
    └── Build info
```

### Rental Key Generator

```
[Generate Rental Key]

Client name (optional): [_____________]
Notes (optional):       [_____________]

[Generate]

Key: DCAM-R-A7B2-X9K4-M3P1

[Copy]  [Generate Another]
```

Keys logged to `dev-keys.log` for your records.

---

## Register Camera Tool (Dev Menu)

For your rental business: register cameras to the local database with full EXIF dump.

### Menu Location

```
Dev Menu > Register Camera
  ├── Via USB
  └── Via Memory Card (Manual fallback)
```

### Registration Flow

#### Via USB (Preferred)

```
1. Connect camera via USB
2. App detects USB device
3. Extract USB Fingerprint:
   - Vendor ID (VID)
   - Product ID (PID)
   - USB Serial Number
4. Mount camera storage
5. Scan for sample video files
6. Run EXIF Dump on samples
7. Auto-fill Camera Profile form
8. User fills manual fields (battery, charger, LUT)
9. Save to custom_cameras.json
```

#### Via Memory Card (Fallback)

```
1. Insert memory card
2. App detects mounted volume
3. Scan for sample video files
4. Run EXIF Dump on samples
5. Auto-fill what we can
6. User manually enters:
   - Serial Number (from camera body label)
   - Battery Type
   - Charger
   - USB Fingerprint (if known)
7. Save to custom_cameras.json
```

### EXIF Dump Fields

```bash
exiftool -j \
  -Make \
  -Model \
  -SerialNumber \
  -CameraSerialNumber \
  -InternalSerialNumber \
  -LensModel \
  -LensSerialNumber \
  -ImageWidth \
  -ImageHeight \
  -VideoFrameRate \
  -CompressorID \
  -ColorSpace \
  -FileType \
  sample.mp4
```

### Camera Profile Form

```
┌─────────────────────────────────────────────────┐
│  Register Camera                                │
├─────────────────────────────────────────────────┤
│                                                 │
│  EXIF Data (auto-filled)                        │
│  ─────────────────────                          │
│  Make:        [Sony_________________]           │
│  Model:       [HDR-CX405____________]           │
│  S/N:         [E35982_______________]           │
│  Resolution:  [1920x1080____________]           │
│  Codec:       [AVCHD/H.264__________]           │
│                                                 │
│  USB Fingerprint (auto if USB connected)        │
│  ─────────────────────                          │
│  Vendor ID:   [054c_____] (Sony)                │
│  Product ID:  [0b8c_____]                       │
│  USB Serial:  [ABC123456____________]           │
│                                                 │
│  Equipment (manual entry)                       │
│  ─────────────────────                          │
│  Battery Type: [NP-FV50_____] [+]               │
│  Charger:      [AC-L200_____] [+]               │
│                                                 │
│  Processing                                     │
│  ─────────────────────                          │
│  Deinterlace:  [x] Yes  [ ] No  [ ] Auto        │
│  LUT:          [None____________] [Browse]      │
│                                                 │
│  Notes                                          │
│  ─────────────────────                          │
│  [Rental Unit #7, purchased 2024-01___________] │
│                                                 │
│           [Cancel]  [Save Camera]               │
└─────────────────────────────────────────────────┘
```

### Custom Camera Storage

Registered cameras saved separately from bundled database:

```
~/.dadcam/
  custom_cameras.json     # Your registered cameras
  battery_types.json      # Battery types you've added
  chargers.json           # Chargers you've added
```

**custom_cameras.json:**
```json
{
  "version": "1.0",
  "cameras": [
    {
      "id": "rental-007",
      "name": "Rental Unit #7",
      "make": "Sony",
      "model": "HDR-CX405",
      "serial_number": "E35982",
      "usb_fingerprint": {
        "vendor_id": "054c",
        "product_id": "0b8c",
        "serial": "ABC123456"
      },
      "resolution": "1920x1080",
      "codec": "h264",
      "battery_type": "NP-FV50",
      "charger": "AC-L200",
      "deinterlace": true,
      "lut": null,
      "notes": "Purchased 2024-01",
      "registered_at": "2024-01-15T10:30:00Z"
    }
  ]
}
```

### USB Detection

**macOS:**
```bash
system_profiler SPUSBDataType -json
```

**Windows:**
```powershell
Get-WmiObject Win32_USBHub | Select-Object DeviceID, Description
```

**Linux:**
```bash
lsusb -v
# or read from /sys/bus/usb/devices/
```

### Build-As-You-Go Dropdowns

Battery Type and Charger dropdowns populated from your entries:

```
[Battery Type dropdown]
├── NP-FV50 (Sony)
├── NP-FV70 (Sony)
├── BP-820 (Canon)
└── [+ Add New...]

[Charger dropdown]
├── AC-L200 (Sony)
├── CG-800 (Canon)
└── [+ Add New...]
```

Clicking [+ Add New...] opens quick-add modal:

```
┌─────────────────────────┐
│  Add Battery Type       │
├─────────────────────────┤
│  Name:  [NP-FV100____]  │
│  Make:  [Sony________]  │
│  Notes: [____________]  │
│                         │
│    [Cancel]  [Add]      │
└─────────────────────────┘
```

### Matching Priority

When importing footage, match against:

```
1. Custom cameras (USB fingerprint)     - 100% confidence
2. Custom cameras (serial number)       - 95% confidence
3. Custom cameras (make + model)        - 80% confidence
4. Bundled database (make + model)      - 80% confidence
5. Bundled database (filename pattern)  - 70% confidence
6. Unknown - generic processing         - 0% confidence
```

Your registered cameras always take priority over bundled database.

---

## Camera Database

### Bundled with App

Ship `canonical.json` (7,500+ cameras) in app bundle.

**Location:** `resources/cameras/canonical.json`

**Updates:** New cameras come with app updates. No remote fetch needed initially.

### Matching Priority

```
1. Exact EXIF Make + Model match    (confidence: 100%)
2. Model variant match              (confidence: 95%)
3. Filename pattern match           (confidence: 80%)
4. Folder structure match           (confidence: 70%)
5. Unknown - use generic defaults   (confidence: 0%)
```

**Unknown cameras still work** - just get generic processing. No blocking.

---

## Implementation Plan

### Phase 9A: Trial System
- [ ] Trial start date storage
- [ ] Machine ID generation
- [ ] Days remaining calculation
- [ ] Trial UI (footer, banner)
- [ ] Soft lock (expired mode)
- [ ] Feature gating

### Phase 9B: License System
- [ ] Key format and validation
- [ ] Key storage
- [ ] Enter key UI
- [ ] License state management

### Phase 9C: Purchase Integration
- [ ] Gumroad product setup
- [ ] "Buy Now" button linking
- [ ] Post-purchase key entry flow

### Phase 9D: Dev Menu (Basic)
- [ ] Secret access (Cmd+Shift+D)
- [ ] Dev menu UI shell
- [ ] Rental key generator
- [ ] Debug tools

### Phase 9E: Camera Database (Bundled)
- [ ] Bundle canonical.json
- [ ] Camera matching (EXIF, patterns)
- [ ] Unknown camera fallback

### Phase 9F: Register Camera Tool
- [ ] USB device detection (macOS, Windows, Linux)
- [ ] USB fingerprint extraction
- [ ] Extended EXIF dump
- [ ] Camera profile form UI
- [ ] Custom cameras storage (custom_cameras.json)
- [ ] Battery/charger dropdowns (build-as-you-go)
- [ ] Memory card fallback flow

### Phase 9G: Camera Matching Integration
- [ ] Merge custom + bundled cameras for matching
- [ ] Priority: custom USB > custom serial > custom make/model > bundled
- [ ] Show matched camera info in clip details

### Phase 9H: Watermark (Optional)
- [ ] Export watermark for unlicensed
- [ ] Watermark design

---

## Summary

### Do This

1. **14-day trial** - Full features, then soft lock
2. **$99 one-time** - Simple, fair, no subscription
3. **Local validation** - No server, works offline
4. **Light protection** - Machine ID, obfuscated checks, watermark
5. **Gumroad for payments** - They handle the hard stuff
6. **Rental keys free** - Pre-generate, hand out liberally
7. **Dev menu** - Your secret tools

### Don't Do This

1. **Online activation** - Breaks offline use, users hate it
2. **Aggressive DRM** - Punishes legit users, doesn't stop pirates
3. **Subscription** - Users hate it for local apps
4. **Complex key schemes** - More complexity = more bugs
5. **Fight every pirate** - Not worth the effort

### Accept This

- Some people will pirate. That's fine.
- Your real customers are parents with precious memories.
- They'll pay $99 happily if the experience is good.
- The pirates weren't going to pay anyway.

---

## Final Philosophy

**Build for the 90% who will pay. Don't punish them to stop the 10% who won't.**

The best anti-piracy is:
- A great product
- A fair price
- Easy purchase
- Good support
- Regular updates

Pirates get a frozen, unsupported version. Customers get a living product.

---

End of Spec
