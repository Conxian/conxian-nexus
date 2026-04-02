-- Kwil Pilot Schema for Conxian Nexus (Full State Replacement)
-- This schema defines the sovereign OLTP layer for Nexus state.

database nexus_pilot;

-- Stacks Blocks: Tracks hard and soft finality blocks
table stacks_blocks {
    hash text primary key,
    height int not null,
    type text not null, -- 'microblock' or 'burn_block'
    state text not null, -- 'soft', 'hard', or 'orphaned'
    created_at text not null, -- ISO-8601
    #idx_height height
}

-- Stacks Transactions: Track transaction finality and data
table stacks_transactions {
    tx_id text primary key,
    block_hash text not null,
    sender text not null,
    payload text,
    created_at text not null,
    #idx_block_hash block_hash
}

-- Nexus State Roots: Tracks the cryptographic state root per block
table nexus_state_roots {
    block_height int primary key,
    state_root text not null,
    created_at text not null -- ISO-8601 write time (updated on reorg/repair)
}

-- MMR Nodes: Persist full MMR tree for O(1) audit restoration
table mmr_nodes {
    pos int primary key,
    hash text not null, -- hex encoded hash
    block_height int not null,
    created_at text not null,
    #idx_block_height block_height
}

-- Actions

-- Action to insert a block
action insert_block($hash, $height, $type, $state, $created_at) public {
    insert into stacks_blocks (hash, height, type, state, created_at)
    values ($hash, $height, $type, $state, $created_at);
}

-- Block state transitions

-- Mark a single block (identified by hash)
action set_block_state_by_hash($hash, $state) public {
    update stacks_blocks
    set state = $state
    where hash = $hash;
}

-- Finality: promote soft blocks up to a given height to hard
action finalize_soft_blocks_through_height($through_height) public {
    update stacks_blocks
    set state = 'hard'
    where height <= $through_height and state = 'soft';
}

-- Reorg: orphan soft blocks from a given height onward
action orphan_soft_blocks_from_height($from_height) public {
    update stacks_blocks
    set state = 'orphaned'
    where height >= $from_height and state = 'soft';
}

-- Action to insert/update state root
action upsert_state_root($block_height, $state_root, $created_at) public {
    insert into nexus_state_roots (block_height, state_root, created_at)
    values ($block_height, $state_root, $created_at)
    on conflict (block_height) do update set
        state_root = $state_root,
        created_at = $created_at;
}

-- Action to insert transactions
action insert_transaction($tx_id, $block_hash, $sender, $payload, $created_at) public {
    insert into stacks_transactions (tx_id, block_hash, sender, payload, created_at)
    values ($tx_id, $block_hash, $sender, $payload, $created_at);
}

-- Action to insert MMR nodes
action insert_mmr_node($pos, $hash, $block_height, $created_at) public {
    insert into mmr_nodes (pos, hash, block_height, created_at)
    values ($pos, $hash, $block_height, $created_at);
}
