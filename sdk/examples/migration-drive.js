// Workstream F4 — driving the privacy-fork migration UX from a wallet.
//
// Pre-fork: `planMigration` previews every shielded note the fork will mint
// for the wallet's accounts, plus the total credited per asset, so the UI can
// show "you'll receive a shielded note worth X".
//
// Post-fork: the wallet scans the notes addressed to it (e.g. via
// `scanGrantedNotes`) and `confirmMigration` checks each planned note landed,
// driving the "migration complete" state.

const { planMigration, confirmMigration } = require('../dist');

const viewingKey = {
  spendPk: '0x' + 'aa'.repeat(32),
  spendSk: '0x' + 'bb'.repeat(32),
  viewPk: '0x' + 'cc'.repeat(32),
  viewSk: '0x' + 'dd'.repeat(32),
};

function main() {
  const accounts = [
    { viewingKey, assetId: 1, balance: 1000n },
    { viewingKey, assetId: 1, balance: 250n, account: 1 },
    { viewingKey, assetId: 2, balance: 7n },
  ];

  // --- Pre-fork preview ---
  const plan = planMigration(accounts);
  console.log('migration plan:');
  console.log('  notes:', plan.notes.length);
  for (const [assetId, total] of Object.entries(plan.totalsByAsset)) {
    console.log(`  asset ${assetId}: will receive ${total.toString()}`);
  }

  // --- Post-fork confirmation ---
  // In production these come from scanGrantedNotes(...). Here we simulate that
  // only the first two notes have landed so far.
  const scannedNotes = plan.notes.slice(0, 2).map((entry) => entry.note);
  const confirmation = confirmMigration(accounts, scannedNotes);

  console.log('migration confirmation:');
  console.log('  confirmed:', confirmation.confirmedCount, 'of', accounts.length);
  console.log('  complete:', confirmation.complete);
  confirmation.results.forEach((result, index) => {
    console.log(`  account ${index}: ${result.confirmed ? 'landed' : 'pending'}`);
  });
}

main();
