
Transactions look like:

type TxData interface {
	txType() byte // returns the type ID
	copy() TxData // creates a deep copy and initializes all fields

	chainID() *big.Int
	accessList() AccessList
	data() []byte
	gas() uint64
	gasPrice() *big.Int
	gasTipCap() *big.Int
	gasFeeCap() *big.Int
	value() *big.Int
	nonce() uint64
	to() *common.Address

	rawSignatureValues() (v, r, s *big.Int)
	setSignatureValues(chainID, v, r, s *big.Int)

	skipAccountChecks() bool

	// effectiveGasPrice computes the gas price paid by the transaction, given
	// the inclusion block baseFee.
	//
	// Unlike other TxData methods, the returned *big.Int should be an independent
	// copy of the computed value, i.e. callers are allowed to mutate the result.
	// Method implementations can use 'dst' to store the result.
	effectiveGasPrice(dst *big.Int, baseFee *big.Int) *big.Int

	encode(*bytes.Buffer) error
	decode([]byte) error
}

Transactions are put into a transaction queue in the sequencer:

type txQueueItem struct {
	tx              *types.Transaction // TxData is one of the fields of Transaction
	txSize          int // size in bytes of the marshalled transaction
  ...
}

- Blocks are built in the createBlock() function in sequencer.go

In order to create a new block, first create the header:
the sequencer gets these inputs to make a block
func ProduceBlockAdvanced(
	l1Header *arbostypes.L1IncomingMessageHeader,
	txes types.Transactions,
	delayedMessagesRead uint64,
	lastBlockHeader *types.Header,
	statedb *state.StateDB,
	chainContext core.ChainContext,
	chainConfig *params.ChainConfig,
	sequencingHooks *SequencingHooks,
	isMsgForPrefetch bool,
)

it makes a header::
	header := createNewHeader(lastBlockHeader, l1Info, state, chainConfig)

then creates a dummy starting tx:
	// Prepend a tx before all others to touch up the state (update the L1 block num, pricing pools, etc)
	startTx := InternalTxStartBlock(chainConfig.ChainID, l1Header.L1BaseFee, l1BlockNum, header, lastBlockHeader)
	txes = append(types.Transactions{types.NewTx(startTx)}, txes...)

iterate through transactions first in first out

call FinalizeBlock
  create header:
		arbitrumHeader := types.HeaderInfo{
			SendRoot:           sendRoot,
			SendCount:          sendCount,
			L1BlockNumber:      nextL1BlockNumber,
			ArbOSFormatVersion: arbosVersion,
		}

then call 		arbitrumHeader.UpdateHeaderWithInfo(header)

Create a new block

func NewBlock(header *Header, txs []*Transaction, uncles []*Header, receipts []*Receipt, hasher TrieHasher) *Block {
	b := &Block{header: CopyHeader(header)}

	// TODO: panic if len(txs) != len(receipts)
	if len(txs) == 0 {
		b.header.TxHash = EmptyTxsHash
	} else {
		b.header.TxHash = DeriveSha(Transactions(txs), hasher)
		b.transactions = make(Transactions, len(txs))
		copy(b.transactions, txs)
	}

	if len(receipts) == 0 {
		b.header.ReceiptHash = EmptyReceiptsHash
	} else {
		b.header.ReceiptHash = DeriveSha(Receipts(receipts), hasher)
		b.header.Bloom = CreateBloom(receipts)
	}

	if len(uncles) == 0 {
		b.header.UncleHash = EmptyUncleHash
	} else {
		b.header.UncleHash = CalcUncleHash(uncles)
		b.uncles = make([]*Header, len(uncles))
		for i := range uncles {
			b.uncles[i] = CopyHeader(uncles[i])
		}
	}

	return b
}

write the created block





