# UI Audit Report: PrimeSwap V2 vs V3
**Date:** March 9, 2026  
**Audited URLs:**
- V3: http://46.225.30.187:4002
- V2: http://46.225.30.187:4000

---

## Executive Summary

This audit compares PrimeSwap V3 (port 4002) against V2 (port 4000) to identify design inconsistencies, branding issues, and establish the target design language for V3.

**Critical Finding:** V3 has significant branding inconsistencies and appears to use a generic Uniswap V3 fork UI without proper Mersennet theming.

---

## 1. BRANDING ANALYSIS

### V3 (Port 4002) - ISSUES IDENTIFIED ⚠️

**Logo/Brand Identity:**
- Uses "PrimeSwap" text branding in header (plain text)
- Missing Mersennet logo/icon
- Page title: "PrimeSwap V3 | Mersennet DEX"
- No visual Prime branding elements

**Color Scheme:**
- Navigation tabs: Dark background with subtle hover states
- Primary buttons: Bright blue/purple gradient (`#5D4FFF` style)
- Background: Deep navy/dark blue (`#1A1B2E` or similar)
- Accent color: Cyan blue (`#00D4FF` style)

**Issues:**
- Logo says "PrimeSwap" but lacks Mersennet diamond logo
- Generic appearance, doesn't establish unique brand identity
- Inconsistent with V2's established branding

### V2 (Port 4000) - REFERENCE DESIGN ✅

**Logo/Brand Identity:**
- **Diamond logo** (purple/pink gradient) + "Prime" text + "Swap" text
- Distinctive multi-color diamond icon (purple, pink, cyan gradient)
- Page title: "PrimeSwap | Mersennet DEX"
- Strong visual brand presence

**Color Scheme:**
- **Primary purple/magenta:** `#7C3AED` to `#EC4899` gradient range
- **Accent cyan:** `#06B6D4` / `#00D4FF`
- **Background:** Dark navy/purple (`#1E1B2E` or similar)
- **Card backgrounds:** Semi-transparent dark with subtle borders
- **Button gradients:** Vibrant purple-to-pink horizontal gradients

**Key Visual Elements:**
- Version switcher badge: "V2" with dropdown to "V1"
- "Mersennet" network indicator button (top right)
- Settings gear icon (top right)
- Hamburger menu (top right)

---

## 2. NAVIGATION STRUCTURE

### V3 Navigation
```
Header:
- Logo (text: "PrimeSwap") [left]
- Tabs: Swap | Pool [center]
- Connect to a wallet [right]
- Theme toggle (moon icon) [right]
- More menu (ellipsis) [right]
```

**Pages:**
- **Swap:** Token swap interface
- **Pool:** Liquidity positions ("Pools Overview" + "New Position" button)

### V2 Navigation
```
Header:
- Logo (diamond + "Prime Swap") [left]
- Mersennet button [right]
- Connect to a wallet [right]
- V2/V1 switcher [right]
- Settings gear [right]
- Menu (hamburger) [right]

Card Tabs:
- Swap | Pool [within card]
```

**Pages:**
- **Swap:** Token swap interface
- **Pool:** "Your Liquidity" section with "Add Liquidity" button

**Key Difference:** V2 uses in-card tabs, V3 uses header-level navigation tabs.

---

## 3. SWAP PAGE COMPARISON

### V3 Swap Page

**Layout:**
- Centered card with "Swap" header
- Settings icon (top right of card)
- Two token input sections:
  - Top: "MRSN" selected, 0.0 input
  - Bottom: "Select a token" button, 0.0 input
- Swap direction arrow between inputs
- "Connect Wallet" button at bottom

**Styling:**
- Card background: Very dark, almost black
- Input backgrounds: Slightly lighter dark
- Primary button: Bright purple gradient
- Token buttons: Purple accent with token icon

**State:** No token selected for output, prompts wallet connection

### V2 Swap Page

**Layout:**
- Card with "Swap | Pool" tabs at top
- "From" section (labeled):
  - 0.0 input (left)
  - "MRSN" token button (right) with gradient icon
- Swap arrow in center
- "To" section (labeled):
  - 0.0 input (left)
  - "Select a token" button (right) in purple gradient
- "Connect Wallet" button at bottom

**Styling:**
- Card has subtle border/glow effect
- Labels: "From" and "To" in muted text
- Inputs: Clean, minimal borders
- Buttons: Bold purple gradients (horizontal, left-to-right)
- Token dropdown has gradient background

**Key Visual Differences:**
- V2 has explicit "From"/"To" labels (better UX)
- V2 uses more prominent gradient buttons
- V2 card has more visual depth (borders/shadows)

---

## 4. POOL PAGE COMPARISON

### V3 Pool Page

**Layout:**
- Header: "Pools Overview" (left)
- "+ New Position" button (top right, purple gradient)
- Empty state card:
  - Inbox icon (line art)
  - "Your liquidity positions will appear here."
  - "Connect a wallet" button (purple gradient)

**Styling:**
- Minimal, clean design
- Large empty state with centered content
- Simple iconography

### V2 Pool Page

**Layout:**
- Card with "Swap | Pool" tabs
- "Add Liquidity" button (full-width, purple gradient)
- "Your Liquidity" section with info icon
- Empty state message: "Connect to a wallet to view your liquidity."
- Helper text: "Don't see a pool you joined? Import it."
- "Migrate V1 Liquidity" link at bottom (cyan)

**Styling:**
- More informative empty state
- Helper text for users who joined pools elsewhere
- Migration CTA for V1 users
- Full-width action button

**Key Differences:**
- V2 provides more guidance and helper text
- V2 includes migration path from V1
- V3 has cleaner but less informative UI

---

## 5. DESIGN LANGUAGE BREAKDOWN

### Typography
**V2:**
- Headers: Bold, sans-serif (likely Inter or similar)
- Body: Medium weight, good contrast
- Links: Cyan accent color

**V3:**
- Similar font family (likely Inter)
- Headers appear lighter weight
- Less typographic hierarchy

### Color Palette

**V2 TARGET PALETTE:**
```css
/* Primary Gradients */
--purple-gradient: linear-gradient(90deg, #7C3AED 0%, #EC4899 100%);
--logo-gradient: linear-gradient(135deg, #7C3AED, #EC4899, #06B6D4);

/* Solid Colors */
--primary-purple: #7C3AED;
--primary-pink: #EC4899;
--accent-cyan: #06B6D4;

/* Backgrounds */
--bg-dark: #1E1B2E;
--card-bg: rgba(30, 27, 46, 0.6);
--card-border: rgba(124, 58, 237, 0.2);

/* Text */
--text-primary: #FFFFFF;
--text-secondary: #A0A0B0;
--text-muted: #6B7280;
```

**V3 CURRENT PALETTE (needs change):**
```css
/* Current - more blue-focused */
--primary-blue-purple: #5D4FFF;
--accent-cyan: #00D4FF;
--bg-dark: #1A1B2E;
--card-bg: #0D0E1A;
```

### Button Styles

**V2 (Target):**
- **Primary buttons:** Horizontal purple-to-pink gradient
- **Border radius:** ~12-16px (rounded but not pill-shaped)
- **Padding:** Generous, ~16px vertical
- **Hover:** Slight brightness increase, smooth transition
- **Shadow:** Subtle purple glow on primary actions

**V3 (Current):**
- **Primary buttons:** Blue-purple solid or gradient
- **Border radius:** Similar ~12-16px
- **Style:** More generic, less distinctive

### Card Styles

**V2 (Target):**
- **Background:** Semi-transparent dark with subtle noise/texture
- **Border:** 1px solid with purple/pink gradient tint at low opacity
- **Border radius:** ~20-24px (very rounded)
- **Shadow:** Subtle purple glow/shadow
- **Padding:** ~24-32px

**V3 (Current):**
- **Background:** Solid very dark
- **Border:** Minimal or none
- **Border radius:** ~16-20px
- **Shadow:** Minimal
- **Less visual depth**

### Interactive Elements

**V2 (Target):**
- Token buttons: Purple gradient backgrounds
- Dropdowns: Smooth purple gradient hover states
- Input focus: Cyan or purple border highlight
- Tab states: Clear active state with bottom border/background

**V3 (Current):**
- More subtle interaction states
- Less use of gradient accents
- Blue-focused rather than purple-pink

---

## 6. CONSOLE ERRORS & TECHNICAL ISSUES

### V3 Errors:
```javascript
// Debug log - RPC failure
Failed to get block number for chainId: 131071 
Error: Failed to send batch call
```
- **Issue:** RPC connection failing for Mersennet (131071)
- **Impact:** Can't fetch on-chain data, blocks/transactions won't update

### V2 Errors:
```javascript
// Failed to fetch token list
Failed to fetch list http://46.225.30.187:4000/primeswap-tokenlist.json 
TypeError: Failed to fetch
```
- **Issue:** Token list JSON not found at expected path
- **Impact:** Custom tokens may not load properly

**Both apps:** React GA (Google Analytics) warnings - not critical

---

## 7. FUNCTIONAL TESTING

### Wallet Connection (Both Apps)
✅ **Working:** 
- Both apps show wallet connection modal
- Modal displays "Install Metamask" option
- Modal has close functionality

❌ **Not Tested:** 
- Actual wallet connection (no Metamask in test environment)
- Post-connection UI changes

### Navigation (Both Apps)
✅ **Working:**
- Swap ↔ Pool navigation smooth
- URLs update correctly with hash routing
- Active tab indicators visible

### Token Selection
❌ **Not Tested:**
- Token selection modals
- Search/filter functionality
- Custom token imports

---

## 8. BRANDING ISSUES - ACTION ITEMS

### 🚨 Critical Issues (V3)

1. **Missing Diamond Logo**
   - V3 shows "PrimeSwap" text only
   - Should use V2's diamond gradient logo + "Prime" + "Swap" text
   - **Action:** Import logo SVG/assets from V2

2. **Color Scheme Mismatch**
   - V3 uses blue-purple theme
   - Should use purple-pink gradient theme from V2
   - **Action:** Update CSS custom properties to match V2 palette

3. **No Mersennet Branding**
   - Missing "Mersennet" network button
   - Missing version indicator
   - **Action:** Add Mersennet button and V3 badge to header

4. **Generic Button Styles**
   - Lacks distinctive purple-pink gradients
   - **Action:** Apply V2 gradient button styles

### ⚠️ Medium Priority Issues

5. **Card Visual Depth**
   - V3 cards too flat, lack borders/shadows
   - **Action:** Add subtle purple borders and glow effects

6. **Typography Hierarchy**
   - Headers less bold than V2
   - **Action:** Increase font weights for headers

7. **Empty State Messaging**
   - V3 pool page less informative
   - **Action:** Add helper text and migration CTAs

### ℹ️ Low Priority Enhancements

8. **Token Button Styling**
   - Could use more prominent gradients
   - **Action:** Add gradient backgrounds to token selectors

9. **Input Labels**
   - V2's "From"/"To" labels improve clarity
   - **Action:** Consider adding explicit labels

10. **Settings Access**
    - V3 has settings in card header, V2 in top nav
    - **Action:** Decide on consistent placement

---

## 9. RECOMMENDED DESIGN DIRECTION FOR V3

### ✅ Keep from V3:
- Header-level navigation tabs (cleaner than in-card tabs)
- "+ New Position" button placement (more prominent)
- Simplified empty states (but add helper text)

### ✅ Adopt from V2:
- **Diamond logo with gradient** (critical branding)
- **Purple-to-pink gradient color scheme** (brand identity)
- **"Mersennet" network indicator** (user orientation)
- **Version badge** ("V3") for clarity
- **Gradient button styles** (visual consistency)
- **Card borders and shadows** (visual depth)
- **Token dropdown gradients** (polish)
- **Helper text and CTAs** (user guidance)

### 🎨 Target V3 Design:

```
Header:
├─ Diamond Logo + "Prime" + "Swap" (gradient)
├─ Navigation Tabs (Swap | Pool) - center
└─ Right Side:
   ├─ Mersennet (network badge)
   ├─ V3 (version badge)
   ├─ Connect to a wallet (gradient button)
   ├─ Settings (gear icon)
   └─ Menu (hamburger)

Main Card:
├─ Border: 1px purple gradient (low opacity)
├─ Shadow: Purple glow
├─ Background: Semi-transparent dark
├─ Padding: 32px
└─ Border radius: 24px

Buttons:
├─ Primary: Horizontal purple-pink gradient
├─ Hover: Brightness +10%, smooth transition
└─ Border radius: 16px

Token Selectors:
├─ Background: Purple gradient (subtle)
├─ Icon: Token logo with border
└─ Dropdown: Cyan highlight on hover
```

---

## 10. IMPLEMENTATION CHECKLIST

### Phase 1: Critical Branding (Do First)
- [ ] Add Mersennet diamond logo SVG to V3 project
- [ ] Update header logo to match V2 (diamond + text)
- [ ] Implement purple-pink gradient CSS variables
- [ ] Update primary button styles with gradients
- [ ] Add "Mersennet" network indicator button
- [ ] Add "V3" version badge

### Phase 2: Visual Refinement
- [ ] Add card borders with gradient tint
- [ ] Implement card shadow/glow effects
- [ ] Update token selector button styles
- [ ] Add gradient backgrounds to dropdowns
- [ ] Improve typography hierarchy (font weights)
- [ ] Update active/hover states with brand colors

### Phase 3: UX Improvements
- [ ] Add "From"/"To" labels to swap inputs (optional)
- [ ] Enhance pool page empty state with helper text
- [ ] Add "Import pool" helper text
- [ ] Improve error messaging
- [ ] Add loading states with brand colors

### Phase 4: Technical Fixes
- [ ] Fix RPC connection for chain ID 131071
- [ ] Add/fix token list JSON endpoint
- [ ] Test wallet connection flow
- [ ] Add proper error boundaries
- [ ] Verify all navigation routes

---

## 11. DESIGN ASSETS NEEDED

### From V2 Codebase:
1. **Mersennet diamond logo** (SVG)
   - Gradient version (purple/pink/cyan)
   - Monochrome version (white)

2. **CSS Custom Properties:**
   - Full color palette
   - Gradient definitions
   - Shadow/glow variables

3. **Component Styles:**
   - Button styles (primary, secondary)
   - Card/panel styles
   - Input/select styles
   - Modal styles

4. **Icon Assets:**
   - Settings gear
   - Network indicator icon
   - Menu hamburger
   - Arrow/swap icons

### Additional Assets:
- "V3" badge/label design
- Loading spinner with brand colors
- Success/error notification styles

---

## 12. CONCLUSION

**Current State:** V3 appears to be a Uniswap V3 fork with minimal Mersennet branding applied. It lacks the distinctive purple-pink gradient theme and diamond logo that define the Mersennet visual identity.

**Target State:** V3 should adopt V2's established branding (logo, colors, gradients) while maintaining its improved header navigation structure. The result will be a modern, on-brand DEX that clearly identifies as part of the Mersennet ecosystem.

**Priority:** Branding updates are CRITICAL. The current V3 UI does not adequately represent Mersennet and could confuse users familiar with V2.

**Estimated Effort:**
- Phase 1 (Critical Branding): 4-8 hours
- Phase 2 (Visual Refinement): 8-12 hours  
- Phase 3 (UX Improvements): 4-6 hours
- Phase 4 (Technical Fixes): 4-8 hours

**Total:** 20-34 hours for complete V3 UI overhaul to match Mersennet branding standards.

---

## APPENDIX A: Color Reference

### V2 Extracted Colors (Target for V3)

**Background Gradients:**
```css
background: linear-gradient(135deg, #1E1B2E 0%, #2D1B3D 100%);
```

**Button Gradients:**
```css
/* Primary CTA */
background: linear-gradient(90deg, #7C3AED 0%, #EC4899 100%);

/* Hover state */
background: linear-gradient(90deg, #8B5CF6 0%, #F472B6 100%);
```

**Border Gradients:**
```css
border: 1px solid transparent;
background-clip: padding-box;
background-image: linear-gradient(#1E1B2E, #1E1B2E), 
                  linear-gradient(90deg, #7C3AED, #EC4899);
background-origin: border-box;
```

**Text Gradients (Logo):**
```css
background: linear-gradient(135deg, #7C3AED, #EC4899, #06B6D4);
-webkit-background-clip: text;
-webkit-text-fill-color: transparent;
```

---

## APPENDIX B: Screenshots Reference

All screenshots captured and available in browser session:
1. V3 Swap page (port 4002)
2. V3 Pool page (port 4002)
3. V3 Wallet modal (port 4002)
4. V2 Swap page (port 4000)
5. V2 Pool page (port 4000)
6. V2 Version tooltip (port 4000)

---

**End of Audit Report**
