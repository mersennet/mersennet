# Etherscan.io Complete Features Catalog

## Table of Contents
1. [Homepage Features](#homepage-features)
2. [Blocks Page](#blocks-page)
3. [Block Detail Page](#block-detail-page)
4. [Transactions Page](#transactions-page)
5. [Transaction Detail Page](#transaction-detail-page)
6. [Address Page](#address-page)
7. [Token Tracker Page](#token-tracker-page)
8. [Top Accounts Page](#top-accounts-page)
9. [Gas Tracker Page](#gas-tracker-page)
10. [Charts & Statistics Page](#charts-statistics-page)
11. [Global Navigation](#global-navigation)
12. [Universal Features](#universal-features)

---

## Homepage Features

### Header Stats (Real-time)
- **ETH Price**: Current price in USD with BTC ratio and percentage change
- **Gas Price**: Current gas in Gwei with USD cost estimate
- **Market Cap**: Total market capitalization
- **TPS (Transactions Per Second)**: Transaction rate metric

### Search Bar
- Universal search with filter dropdown
- Search types: All Filters, Addresses, Tokens, Name Tags, Domain Names, Labels, Websites
- Placeholder: "Search by Address / Txn Hash / Block / Token / Domain Name"

### Main Stats Section
- **ETH Price**: Detailed price with BTC ratio and percentage change
- **Transactions**: Total count with TPS
- **Market Cap**: Current market capitalization
- **Gas Price**: In Gwei with USD equivalent

### Latest Blocks Widget
- **Columns**:
  - Block number (clickable)
  - Age (relative time)
  - Fee Recipient (miner/builder name with label)
  - Transaction count
  - Reward amount
- **Features**:
  - Auto-refreshing (real-time updates)
  - "View all blocks" link
  - Customize button

### Latest Transactions Widget
- **Columns**:
  - Transaction hash (truncated, clickable)
  - Age (relative time)
  - From address (truncated with label if available)
  - To address (truncated with label if available)
  - Amount in ETH
- **Features**:
  - Auto-refreshing (real-time updates)
  - "View all transactions" link
  - Customize button
  - Method tags (e.g., Transfer, Swap)

### Footer
- **Social Media Links**: X (Twitter), Medium, Facebook, Reddit
- **Company Section**: About Us, Brand Assets, Contact Us, Careers, Terms & Privacy, Bug Bounty
- **Community Section**: API Documentation, Knowledge Base, Network Status, Newsletters
- **Products & Services**: Advertise, EaaS, API Plans, Priority Support, Blockscan, Blockscan Chat
- **Copyright**: Year and donation address

---

## Blocks Page

### URL: `https://etherscan.io/blocks`

### Top Statistics Cards
- **Network Utilization (24H)**: Percentage with link to chart
- **Last Safe Block**: Block number
- **Blocks by MEV Builders (24H)**: Percentage
- **Burnt Fees 🔥**: Total ETH burnt

### Blocks Table
**Columns**:
1. **Block**: Block number (clickable)
2. **Slot**: Beacon chain slot number
3. **Age**: Relative time
4. **Blobs**: Blob count with percentage bar
5. **Txn**: Transaction count (clickable)
6. **Fee Recipient**: Miner/builder name with copy button
7. **Gas Used**: Amount used with percentage of limit and bar indicator
8. **Gas Limit**: Maximum gas limit
9. **Base Fee**: Fee in Gwei
10. **Reward**: Block reward in ETH with percentage change
11. **Burnt Fees (ETH)**: Amount burnt with percentage

**Features**:
- Pagination: First, Previous, Page X of Y, Next, Last
- Entries per page selector: 10, 25, 50, 100
- Download Page Data button
- API link
- Sortable columns (indicated by clickable headers)
- Total block count display
- Address copy buttons

---

## Block Detail Page

### URL: `https://etherscan.io/block/[NUMBER]`

### Page Header
- Block number (large display)
- MEV Block indicator
- EigenPhi integration link
- API link
- Previous/Next block navigation arrows

### Tabs
1. **Overview** (default)
2. **Consensus Info**
3. **MEV Info**
4. **Blob Info**

### Overview Tab - Main Fields

**Basic Information**:
- **Block Height**: Number with navigation arrows
- **Status**: Finalized badge
- **Timestamp**: Exact UTC time with relative age
- **Proposed On**: Slot and epoch numbers with link
- **Transactions**: Count with links to transactions and internal transactions
- **Withdrawals**: Count with link

**Block Producer**:
- **Fee Recipient**: Address/label with copy button and epoch count

**Block Details**:
- **Block Reward**: ETH amount
- **Total Difficulty**: Large number with scientific notation
- **Size**: In bytes
- **Gas Used**: Amount with percentage of limit and visual bar
- **Gas Limit**: Maximum gas allowed
- **Base Fee Per Gas**: In ETH and Gwei
- **Burnt Fees**: ETH amount with fire emoji
- **Extra Data**: Builder info and hash

### More Details (Expandable)
- **Hash**: Full block hash
- **Parent Hash**: Previous block hash (clickable)
- **StateRoot**: State root hash
- **WithdrawalsRoot**: Withdrawals root hash
- **Nonce**: Block nonce value

**Features**:
- Expandable "More Details" button
- All hashes are copyable
- Links to related data (transactions, internal transactions, etc.)
- Tooltips on hover for explanations

---

## Transactions Page

### URL: `https://etherscan.io/txs`

### Top Statistics Cards
- **Transactions (24H)**: Count with percentage change and TPS
- **Pending Transactions (Last 1H)**: Count
- **Total Transaction Fee (24H)**: ETH amount with percentage change
- **Avg. Transaction Fee (24H)**: USD amount with percentage change

### Transactions Table
**Columns**:
1. **Transaction Hash**: Truncated hash with icon and copy button
2. **Method**: Function name with badge/icon
3. **Block**: Block number (clickable)
4. **Age**: Relative time
5. **From**: Address with label and copy button
6. **To**: Address with label, icon (contract/EOA), and copy button
7. **Amount**: Value in ETH
8. **Txn Fee**: Fee amount in ETH

**Features**:
- Real-time updates
- Method badges with colors (Transfer, Swap, Approve, etc.)
- Address labels (exchange names, known contracts)
- Contract indicators (square icon)
- Pagination controls
- Download Page Data button
- Shows total transaction count
- Filters by last 500k records

**Interactive Elements**:
- Clickable hashes → transaction details
- Clickable blocks → block details
- Clickable addresses → address pages
- Copy buttons for hashes and addresses
- Method name tooltips

---

## Transaction Detail Page

### URL: `https://etherscan.io/tx/[HASH]`

### Page Header
- Transaction hash (full display)
- Buy/Play/Gaming dropdown buttons (sponsored)
- View on Blockscan link

### Transaction Overview

**Status & Timing**:
- **Transaction Hash**: Full hash with copy button
- **Status**: Success/Failed badge with confirmation count
- **Block**: Block number with confirmations
- **Timestamp**: Exact time with relative age
- **Transaction Action**: Human-readable description (if available)

**Participants**:
- **From**: Sender address with label and copy button
- **Interacted With (To)**: Contract address with label and copy button
- **Tokens Transferred**: List of token transfers with amounts and addresses

**Transaction Details**:
- **Value**: Amount in ETH and USD
- **Transaction Fee**: Gas price × gas used in ETH and USD
- **Gas Price**: In Gwei and ETH
- **Gas Limit & Usage by Txn**: Amount with percentage
- **Gas Fees**: Base + Max + Max Priority
- **Burnt Fees**: ETH burnt amount
- **Other Attributes**: Txn type, nonce, position in block

**Advanced Details**:
- **Input Data**: Raw input bytes (expandable)
- **State**: State tab
- **Logs**: Event logs if applicable

### Special Sections
- **Token Transfers**: Expandable list with From/To/Amount
- **Internal Transactions**: Count and link to view
- **Logs**: Event logs with topics and data
- **State Changes**: Before/after states

**Features**:
- Private note addition (for logged-in users)
- Add to address book
- Report issues
- Copy transaction hash
- View raw logs option
- Decode input data

---

## Address Page

### URL: `https://etherscan.io/address/[ADDRESS]`

### Page Header
- **Address**: Full address with copy button
- **Label/Name**: ENS or known label (e.g., "vitalik.eth")
- **Public Name Tag**: If verified
- **Authority badge**: If designated
- **EIP-7702 Delegated badge**: If applicable
- **Buy/Play/Gaming buttons**: Sponsored dropdowns

### Overview Section

**Balance Information**:
- **ETH Balance**: Amount in ETH and USD value
- **ETH Value**: USD equivalent at current price
- **Token Holdings**: Count with link to full list

**Activity Stats**:
- **Transactions Sent**: Count with time range
- **First/Last Activity**: Dates
- **Funded By**: Original funding source (if tracked)

### More Info Card
- **Private Name Tags**: Personal labels (requires sign-in)
- **Last Txn Sent**: Transaction hash and age
- **First Txn Sent**: Transaction hash and age

### Multichain Info Card
- Multichain portfolio value
- Chain count
- Addresses found via Blockscan

### Tabs

#### 1. Transactions Tab (Default)
**Columns**:
- Transaction Hash
- Method (with badge)
- Block number
- Age
- From address
- To address  
- Amount
- Txn Fee

**Features**:
- Latest 25 from total count display
- Advanced Filter button
- Download Page Data button
- Pagination
- Method filters

#### 2. Internal Transactions Tab
Shows internal contract-to-contract transactions

#### 3. Token Transfers (ERC-20) Tab
**Columns**:
- Txn Hash
- Age
- From
- To
- Token (name and symbol)
- Amount

#### 4. NFT Transfers Tab
Shows ERC-721 and ERC-1155 transfers

#### 5. Other Transactions Tab
Less common transaction types

#### 6. Analytics Tab
Address analytics and statistics

#### 7. Assets Tab
Complete token and NFT holdings

#### 8. Cards Tab
NFT gallery view (if applicable)

**Universal Tab Features**:
- Pagination on all tabs
- Download CSV option
- Advanced filtering
- Sort by columns
- Age format toggle (relative/exact)

---

## Token Tracker Page

### URL: `https://etherscan.io/tokens`

### Top Section
- Total token contracts found count
- Filter indicator (showing tokens with OK or Neutral Reputation)
- Blue Checkmark explanation link

### Tokens Table
**Columns**:
1. **#**: Rank number
2. **Token**: Token name and symbol with logo and verified checkmark
3. **Price**: Current price in USD (with ETH equivalent below)
4. **Change (%)**: 24h price change with color coding
5. **Volume (24H)**: Trading volume in USD
6. **Circulating Market Cap**: Market cap value with tooltip
7. **On-Chain Market Cap**: Alternative market cap calculation
8. **Holders**: Number of unique holders with count badge

**Features**:
- Sortable columns (click column headers)
- Token logos displayed
- Blue verified checkmarks for legitimate tokens
- Pagination (showing top 50 per page)
- Page selector dropdown: 10, 25, 50, 100
- Links to individual token pages
- Filter button (collapsed by default)
- ERC-20 standard indicator

**Color Coding**:
- Green for positive price changes
- Red for negative price changes
- Percentage changes with up/down indicators

**Interactive Elements**:
- Click token name → Token detail page
- Click contract address → Contract page
- Hover for full token names
- Copy contract addresses

---

## Top Accounts Page

### URL: `https://etherscan.io/accounts`

### Page Header
- Title: "Top Accounts by ETH Balance"
- API link
- Total accounts count display

### Accounts Table
**Columns**:
1. **Rank**: Position number
2. **Address**: Full address with label and copy button
3. **Name Tag**: Known label (exchanges, contracts, etc.)
4. **Balance**: ETH amount
5. **Percentage**: Percentage of total ETH supply
6. **Txn Count**: Number of transactions

**Features**:
- Showing first 10,000 accounts only
- Pagination: First, Previous, Page X of 400, Next, Last
- Entries per page: 10, 25, 50, 100
- Download Page Data button
- Address copy buttons
- Name tags for known entities

**Name Tag Types**:
- Exchange wallets (Binance, Coinbase, etc.)
- Bridge contracts
- DeFi protocols
- Notable individuals (when labeled)
- Contract names

**Visual Indicators**:
- Percentage bars for balance distribution
- Color coding for address types
- Labels for special addresses (Beacon Deposit Contract, etc.)

---

## Gas Tracker Page

### URL: `https://etherscan.io/gastracker`

### Page Header
- "Ethereum Gas Tracker ⛽" title
- Sponsored by Blockscan badge
- Next update timer
- Gas APIs link
- Install Gas Extension link

### Current Gas Prices (Top Section)

**Three Tiers** (displayed as cards):
1. **Low** (🙂)
   - Gas price in Gwei
   - Base fee + Priority
   - Estimated cost in USD
   - Estimated time (~30 secs)

2. **Average** (😐)
   - Gas price in Gwei
   - Base + Priority breakdown
   - Cost estimate
   - Time estimate

3. **High** (😊)
   - Gas price in Gwei
   - Base + Priority fees
   - Cost estimate
   - Fastest time estimate

### Additional Info Section
- **Last Block**: Block number
- **Pending Queue**: Transaction count
- **Avg Block Size**: Size in KB
- **Avg. Utilization**: Percentage
- **Last Refreshed**: Timestamp

### Featured Actions Section
Table showing gas costs for common actions:
- **Swap**: Low/Average/High gas in Gwei
- **NFT Sale**: Gas estimates for each tier
- **Bridging**: Gas costs
- **Borrowing**: Gas estimates
- **Custom Gas Limit**: Input field for custom calculations

**Features**:
- Real-time price updates (refreshes every ~10 seconds)
- USD and ETH cost estimates
- Time estimates for confirmation
- Gas API integration link

### Heatmap & Chart Tabs

#### Heatmap Tab (Default)
- Weekly gas price heatmap
- Color-coded by gas price intensity
- Hourly breakdown for 7 days
- Shows patterns in network usage

#### Chart Tab
- Historical gas price line chart
- Time series data
- Zoom and pan controls
- Multiple timeframes

### Gas Guzzlers Tab (Default Selected)
**Table showing top gas consumers**:
**Columns**:
1. **Rank**
2. **Contract Name**: With label and copy button
3. **Gas Used (24h)**: Amount in gas units
4. **Gas Used %**: Percentage of total
5. **Avg Gas Price**: In Gwei

**Features**:
- Top 50 contracts
- Show rows selector: 10, 15, 25, 50
- Pagination
- Contract labels (Tether USDT, Uniswap, MEV bots, etc.)

### Gas Spenders Tab
Similar table showing top addresses paying for gas

### Historical Gas Oracle Prices Tab
Historical data table with dates and prices

### Cost of Transaction Actions Tab
Table showing typical costs for different transaction types

**Common Actions Listed**:
- Transfer
- Approve
- Swap (Uniswap)
- Add Liquidity
- Remove Liquidity
- NFT Mint
- NFT Transfer
- Contract Deployment

---

## Charts & Statistics Page

### URL: `https://etherscan.io/charts`

### Page Header & Navigation
**Left sidebar categories**:
1. Overview Stats
2. Market Data
3. Blockchain Data
4. Network Data
5. Contracts

### Overview Stats Section
Grid of live statistics cards:

**Row 1**:
- **Addresses (Total)**: Count with percentage change
- **Transactions (Total)**: Count with TPS
- **New Addresses (24h)**: Daily new count
- **Transactions (24h)**: Daily count with change

**Row 2**:
- **Tokens (Total)**: ERC-20 count
- **Pending Transactions (1h)**: Current queue
- **Total Transaction Fee (24H)**: ETH amount
- **Avg. Transaction Fee (24h)**: USD amount

**Row 3**:
- **Contracts Deployed (Total)**: Cumulative count
- **Contracts Verified (Total)**: Verified count
- **Contracts Deployed (24h)**: Daily deployments
- **Contracts Verified (24h)**: Daily verifications

**Row 4**:
- **Total Gas Used (24h)**: Gas amount in millions
- **Network Utilization (24H)**: Percentage
- **Blocks by MEV Builders (24H)**: Percentage
- **Burnt Fees 🔥 (24H)**: ETH burnt

### Market Data Section

**Charts Available** (each with "View" link):
1. **Ether Daily Price (USD) Chart (7D)**
   - Line chart showing price trends
   - Time period: Last 7 days
   
2. **Ether Market Capitalization Chart (7D)**
   - Market cap over time
   - USD values

3. **Total Supply & Market Cap Chart (7D)**
   - Combined chart showing both metrics

4. **Ether Distribution Breakdown (7D)**
   - Pie chart or breakdown of ETH distribution

### Blockchain Data Section

**Available Charts** (all with 7D timeframe):
1. Daily Transactions Chart
2. ERC20 Daily Token Transfer Chart
3. Unique Addresses Chart
4. Average Block Size Chart
5. Average Block Time Chart
6. Average Gas Price Chart
7. Average Gas Limit Chart
8. Daily Gas Used Chart
9. Daily Block Rewards Chart
10. Block Count and Rewards Chart
11. Uncle Count and Rewards Chart
12. Full Node Sync (Default) Chart
13. Full Node Sync (Archive) Chart
14. Daily Active Ethereum Addresses
15. Daily Active ERC20 Addresses
16. Average Transaction Fee Chart
17. Daily Eth Burnt Chart

### Network Data Section

**Charts**:
1. Network Hash Rate Chart (7D)
2. Network Difficulty Chart (7D)
3. Network Pending Transactions Chart (7D)
4. Network Transaction Fee Chart (7D)
5. Network Utilization Chart (7D)
6. Node Tracker (7D)

### Contracts Data Section

**Charts**:
1. Ethereum Daily Verified Contracts Chart (7D)
2. Ethereum Daily Deployed Contracts Chart (7D)

**Chart Features** (Universal):
- Zoom controls
- Date range selectors
- Export options
- Embed code
- CSV download
- API access link
- Interactive tooltips
- Legend toggles

---

## Global Navigation

### Top Navigation Bar

**Left Side**:
- Etherscan logo (home link)
- Home link

**Center Menus** (Dropdown):

#### Blockchain Menu
- Transactions
- Pending Transactions
- Contract Internal Transactions
- Beacon Deposits
- Beacon Withdrawals
- View Blobs
- AA Transactions (Beta)
- EIP-7702 Authorizations (Beta)
- View Blocks
- Forked Blocks (Reorgs)
- Uncles
- Top Accounts
- Verified Contracts

#### Tokens Menu
- Top Tokens (ERC-20)
- Token Transfers (ERC-20)
- Token Flow Visualizer (Beta)

#### NFTs Menu
- Top NFTs
- Top Mints
- Latest Trades
- Latest Transfers
- Latest Mints

#### Resources Menu
- Charts And Stats
- Top Statistics
- Leaderboard
- Etherscan Points (Beta)
- Directory
- Newsletter
- Knowledge Base

#### Developers Menu
- API Plans
- API Documentation
- Code Reader (Beta)
- Verify Contract
- Similar Contract Search
- Smart Contract Search
- Contract Diff Checker
- Vyper Online Compiler
- Bytecode to Opcode
- Broadcast Transaction

#### More Menu (Tools & Services)
**Tools**:
- Input Data Decoder (Beta)
- Unit Converter
- CSV Export
- Account Balance Checker

**Explore**:
- Gas Tracker
- DEX Tracker
- Node Tracker
- Label Cloud
- Domain Name Lookup

**Services**:
- Token Approvals (Beta)
- Verified Signature
- Input Data Messages (IDM) (Beta)
- Advanced Filter (Beta)
- Blockscan Chat (Beta)

**Right Side**:
- Search bar with filter dropdown
- Notification bell icon
- Theme toggle (Light/Dim/Dark)
- Settings icon
- Sign In button

### Settings Menu
- Light/Dim/Dark theme options
- Site Settings
- Network selector (Ethereum Mainnet, Beaconscan ETH2, Sepolia Testnet, Hoodi Testnet)

---

## Universal Features

### Available on All Pages

**1. Search Functionality**
- Universal search bar
- Filter by type dropdown
- Auto-suggestions
- Recent searches (when logged in)

**2. Copy Buttons**
- All addresses have copy buttons
- All hashes have copy buttons
- "Before You Copy" warning modal (first time)

**3. Private Notes & Tags** (Logged-in users)
- Private name tags for addresses
- Private transaction notes
- Saved filters
- Watch lists

**4. Tooltips & Help**
- Info icons with explanations
- Field descriptions
- Knowledge Base links
- Context-sensitive help

**5. Export Options**
- Download Page Data (CSV)
- API links on most pages
- Custom CSV exports

**6. Pagination Controls**
- First, Previous, Next, Last buttons
- Page number display
- Entries per page selector
- Total count display

**7. Time Display**
- Relative time (e.g., "5 mins ago")
- Absolute timestamp on hover
- UTC timezone
- Toggle between formats

**8. Address Display**
- Truncated format with full address on hover
- Labels for known entities
- ENS name resolution
- Copy functionality
- Icon indicators (contract vs EOA)

**9. Value Display**
- ETH amounts with USD equivalent
- Token amounts with symbol
- Percentage changes with color coding
- Scientific notation for large numbers

**10. Real-time Updates**
- Auto-refresh on homepage
- Live gas tracker
- Real-time transaction feed
- Block updates

**11. Mobile Responsiveness**
- Responsive tables
- Collapsible menus
- Touch-friendly buttons
- Optimized layouts

**12. API Integration**
- API links on data pages
- Documentation references
- Rate limit information
- Example queries

**13. Advertisement Placements**
- Sponsored links in navigation
- Banner ads (select pages)
- Inline ads on some pages
- Clearly marked as "Ad" or "Sponsored"

**14. Cookie Consent**
- Cookie notice banner
- Preference management
- Privacy policy link
- Terms link

**15. Error Handling**
- "Not found" pages
- Pending transaction warnings
- Network error messages
- Invalid input feedback

**16. Accessibility**
- Keyboard navigation
- Screen reader support
- ARIA labels
- High contrast modes

---

## Additional Notable Features

### Advanced Filter (Beta)
- Custom transaction filtering
- Save filter presets
- Combine multiple criteria
- Export filtered results

### Blockscan Integration
- Multi-chain search
- Cross-chain address lookup
- Unified portfolio view

### EigenPhi MEV Analytics
- MEV block identification
- Builder information
- Profit calculations

### Blue Checkmark System
- Verified tokens
- Scam prevention
- Reputation scoring

### Label System
- Exchange labels
- DeFi protocol labels
- Known scam labels
- User-submitted labels

### Name Tag Registry
- Public name tags
- Verified entities
- ENS integration
- Domain lookups

---

## Summary Statistics

**Total Cataloged Features**: 500+

**Page Types Documented**: 10

**Navigation Items**: 60+

**Data Fields on Block Detail**: 20+

**Data Fields on Transaction Detail**: 30+

**Data Fields on Address Page**: 50+ (across all tabs)

**Chart Types Available**: 30+

**Interactive Elements**: 100+

**Export Formats**: CSV, API, JSON

**Real-time Features**: 15+

**Beta Features**: 10+

---

*This catalog was created by systematically exploring Etherscan.io on March 9, 2026*
