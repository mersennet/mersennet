const assert = require('node:assert/strict');
const test = require('node:test');

const { reconstructPortfolio, defaultNullifierDeriver } = require('../dist');

function note(value, assetId, tag) {
  return {
    value: BigInt(value),
    assetId,
    ownerPk: '0x' + '11'.repeat(32),
    rho: '0x' + tag.toString(16).padStart(64, '0'),
    psi: '0x' + '66'.repeat(32),
  };
}

test('reconstructPortfolio sums unspent notes per asset', () => {
  const notes = [note(100, 1, 1), note(50, 1, 2), note(7, 2, 3)];
  const portfolio = reconstructPortfolio(notes);
  assert.equal(portfolio.perAsset[1], 150n);
  assert.equal(portfolio.perAsset[2], 7n);
  assert.equal(portfolio.unspentNoteCount, 3);
  assert.equal(portfolio.spentNoteCount, 0);
  assert.equal(portfolio.totalNoteCount, 3);
});

test('reconstructPortfolio excludes spent notes from the balance', () => {
  const spentNote = note(100, 1, 1);
  const liveNote = note(50, 1, 2);
  const spentNullifier = defaultNullifierDeriver(spentNote);

  const portfolio = reconstructPortfolio([spentNote, liveNote], {
    spentNullifiers: [spentNullifier],
  });

  assert.equal(portfolio.perAsset[1], 50n);
  assert.equal(portfolio.unspentNoteCount, 1);
  assert.equal(portfolio.spentNoteCount, 1);
  const spentEntry = portfolio.notes.find((n) => n.nullifier === spentNullifier);
  assert.ok(spentEntry);
  assert.equal(spentEntry.spent, true);
});

test('reconstructPortfolio de-duplicates notes by nullifier', () => {
  const n = note(100, 1, 1);
  const portfolio = reconstructPortfolio([n, n, n]);
  assert.equal(portfolio.perAsset[1], 100n);
  assert.equal(portfolio.totalNoteCount, 1);
  assert.equal(portfolio.unspentNoteCount, 1);
});

test('reconstructPortfolio honours a custom isSpent predicate', () => {
  const notes = [note(100, 1, 1), note(50, 1, 2)];
  const portfolio = reconstructPortfolio(notes, {
    isSpent: () => true,
  });
  assert.equal(Object.keys(portfolio.perAsset).length, 0);
  assert.equal(portfolio.unspentNoteCount, 0);
  assert.equal(portfolio.spentNoteCount, 2);
});

test('spent-nullifier matching is case-insensitive and 0x-tolerant', () => {
  const n = note(100, 1, 1);
  const nullifier = defaultNullifierDeriver(n).slice(2).toUpperCase();
  const portfolio = reconstructPortfolio([n], { spentNullifiers: [nullifier] });
  assert.equal(portfolio.unspentNoteCount, 0);
  assert.equal(portfolio.spentNoteCount, 1);
});
