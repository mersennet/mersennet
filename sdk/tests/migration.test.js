const assert = require('node:assert/strict');
const test = require('node:test');

const {
  deriveMigrationNote,
  matchesMigrationNote,
  defaultNoteCommitment,
  planMigration,
  confirmMigration,
} = require('../dist');

const viewingKey = {
  spendPk: '0x' + 'aa'.repeat(32),
  spendSk: '0x' + 'bb'.repeat(32),
  viewPk: '0x' + 'cc'.repeat(32),
  viewSk: '0x' + 'dd'.repeat(32),
};

test('deriveMigrationNote is deterministic and carries the balance', () => {
  const a = deriveMigrationNote({ viewingKey, assetId: 1, balance: 1000n });
  const b = deriveMigrationNote({ viewingKey, assetId: 1, balance: 1000n });
  assert.equal(a.note.value, 1000n);
  assert.equal(a.note.assetId, 1);
  assert.equal(a.note.ownerPk, viewingKey.spendPk);
  assert.equal(a.note.rho, b.note.rho);
  assert.equal(a.note.psi, b.note.psi);
  assert.equal(a.commitment, b.commitment);
  assert.notEqual(a.note.rho, a.note.psi);
});

test('different account/asset yield distinct blinding factors', () => {
  const base = deriveMigrationNote({ viewingKey, assetId: 1, balance: 1n });
  const otherAsset = deriveMigrationNote({ viewingKey, assetId: 2, balance: 1n });
  const otherAccount = deriveMigrationNote({ viewingKey, assetId: 1, balance: 1n, account: 1 });
  assert.notEqual(base.note.rho, otherAsset.note.rho);
  assert.notEqual(base.note.rho, otherAccount.note.rho);
  assert.notEqual(base.commitment, otherAsset.commitment);
});

test('matchesMigrationNote recognizes the derived note', () => {
  const params = { viewingKey, assetId: 5, balance: 4242n };
  const { note } = deriveMigrationNote(params);
  assert.equal(matchesMigrationNote(note, params), true);

  const tampered = { ...note, value: note.value + 1n };
  assert.equal(matchesMigrationNote(tampered, params), false);

  const wrongRho = { ...note, rho: '0x' + '00'.repeat(32) };
  assert.equal(matchesMigrationNote(wrongRho, params), false);
});

test('matchesMigrationNote tolerates hex case/0x differences in ownerPk', () => {
  const params = { viewingKey, assetId: 5, balance: 4242n };
  const { note } = deriveMigrationNote(params);
  const upper = { ...note, ownerPk: note.ownerPk.toUpperCase().replace('0X', '0x') };
  assert.equal(matchesMigrationNote(upper, params), true);
});

test('custom commitmentHasher is honored', () => {
  const fixed = () => '0x' + 'ff'.repeat(32);
  const out = deriveMigrationNote({ viewingKey, assetId: 1, balance: 1n, commitmentHasher: fixed });
  assert.equal(out.commitment, '0x' + 'ff'.repeat(32));
  assert.notEqual(out.commitment, defaultNoteCommitment(out.note));
});

test('planMigration previews notes and totals per asset', () => {
  const accounts = [
    { viewingKey, assetId: 1, balance: 1000n },
    { viewingKey, assetId: 1, balance: 250n, account: 1 },
    { viewingKey, assetId: 2, balance: 7n },
  ];
  const plan = planMigration(accounts);
  assert.equal(plan.notes.length, 3);
  assert.equal(plan.totalsByAsset[1], 1250n);
  assert.equal(plan.totalsByAsset[2], 7n);
});

test('confirmMigration detects landed and missing notes post-fork', () => {
  const accounts = [
    { viewingKey, assetId: 1, balance: 1000n },
    { viewingKey, assetId: 2, balance: 7n },
  ];
  const plan = planMigration(accounts);
  const scanned = [plan.notes[0].note]; // only the first note landed

  const confirmation = confirmMigration(accounts, scanned);
  assert.equal(confirmation.confirmedCount, 1);
  assert.equal(confirmation.complete, false);
  assert.equal(confirmation.results[0].confirmed, true);
  assert.equal(confirmation.results[1].confirmed, false);

  const full = confirmMigration(accounts, plan.notes.map((n) => n.note));
  assert.equal(full.complete, true);
  assert.equal(full.confirmedCount, 2);
});
