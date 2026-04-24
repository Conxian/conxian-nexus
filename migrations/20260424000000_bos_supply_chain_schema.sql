-- [CON-504] BOS supply-chain checkpoint/proof schema, derived read models, and invariant triggers

CREATE TABLE IF NOT EXISTS sc_checkpoint_events (
    event_row_id BIGSERIAL PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    ingest_seq BIGINT NOT NULL,
    event_id TEXT NOT NULL,
    publisher TEXT NOT NULL,
    kind TEXT NOT NULL,
    sequence BIGINT NOT NULL,
    subject_id TEXT,
    checkpoint_type TEXT,
    checkpoint_at TIMESTAMPTZ,
    payload_json JSONB NOT NULL,
    payload_hash TEXT NOT NULL,
    sigs_json JSONB NOT NULL DEFAULT '[]'::jsonb,
    commitments_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    observed_envelope_hash TEXT,
    leaf_hash TEXT NOT NULL,
    stream_prev_event_id TEXT,
    stream_chain_hash TEXT,
    inserted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_checkpoint_events_dataset_ingest_unique UNIQUE (dataset_id, ingest_seq),
    CONSTRAINT sc_checkpoint_events_dataset_event_unique UNIQUE (dataset_id, event_id),

    CONSTRAINT sc_checkpoint_events_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_checkpoint_events_event_id_not_empty CHECK (btrim(event_id) <> ''),
    CONSTRAINT sc_checkpoint_events_publisher_not_empty CHECK (btrim(publisher) <> ''),
    CONSTRAINT sc_checkpoint_events_kind_not_empty CHECK (btrim(kind) <> ''),
    CONSTRAINT sc_checkpoint_events_payload_hash_not_empty CHECK (btrim(payload_hash) <> ''),
    CONSTRAINT sc_checkpoint_events_leaf_hash_not_empty CHECK (btrim(leaf_hash) <> ''),

    CONSTRAINT sc_checkpoint_events_ingest_seq_positive CHECK (ingest_seq > 0),
    CONSTRAINT sc_checkpoint_events_sequence_non_negative CHECK (sequence >= 0),

    CONSTRAINT sc_checkpoint_events_kind_valid CHECK (
        kind IN (
            'supplychain.checkpoint.v1',
            'supplychain.gap.v1',
            'supplychain.anomaly.v1'
        )
    ),

    CONSTRAINT sc_checkpoint_events_event_id_shape CHECK (event_id ~* '^[0-9a-f]{64}$'),
    CONSTRAINT sc_checkpoint_events_payload_hash_shape CHECK (payload_hash ~* '^[0-9a-f]{64}$'),
    CONSTRAINT sc_checkpoint_events_leaf_hash_shape CHECK (leaf_hash ~* '^[0-9a-f]{64}$'),
    CONSTRAINT sc_checkpoint_events_stream_prev_event_id_shape CHECK (
        stream_prev_event_id IS NULL OR stream_prev_event_id ~* '^[0-9a-f]{64}$'
    ),
    CONSTRAINT sc_checkpoint_events_stream_chain_hash_shape CHECK (
        stream_chain_hash IS NULL OR stream_chain_hash ~* '^[0-9a-f]{64}$'
    ),
    CONSTRAINT sc_checkpoint_events_observed_envelope_hash_shape CHECK (
        observed_envelope_hash IS NULL OR observed_envelope_hash ~* '^[0-9a-f]{64}$'
    ),

    CONSTRAINT sc_checkpoint_events_subject_id_not_blank CHECK (
        subject_id IS NULL OR btrim(subject_id) <> ''
    ),
    CONSTRAINT sc_checkpoint_events_checkpoint_type_not_blank CHECK (
        checkpoint_type IS NULL OR btrim(checkpoint_type) <> ''
    ),

    CONSTRAINT sc_checkpoint_events_payload_json_object CHECK (jsonb_typeof(payload_json) = 'object'),
    CONSTRAINT sc_checkpoint_events_sigs_json_array CHECK (jsonb_typeof(sigs_json) = 'array'),
    CONSTRAINT sc_checkpoint_events_commitments_json_object CHECK (jsonb_typeof(commitments_json) = 'object'),

    CONSTRAINT sc_checkpoint_events_checkpoint_shape CHECK (
        kind <> 'supplychain.checkpoint.v1'
        OR (
            subject_id IS NOT NULL
            AND checkpoint_type IS NOT NULL
            AND checkpoint_at IS NOT NULL
        )
    ),

    CONSTRAINT sc_checkpoint_events_stream_prev_not_self CHECK (
        stream_prev_event_id IS NULL OR stream_prev_event_id <> event_id
    ),

    CONSTRAINT fk_sc_checkpoint_events_stream_prev
        FOREIGN KEY (dataset_id, stream_prev_event_id)
        REFERENCES sc_checkpoint_events (dataset_id, event_id)
        ON DELETE RESTRICT
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_sc_checkpoint_events_stream_sequence_subject
    ON sc_checkpoint_events (dataset_id, publisher, subject_id, sequence)
    WHERE subject_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS ux_sc_checkpoint_events_stream_sequence_no_subject
    ON sc_checkpoint_events (dataset_id, publisher, sequence)
    WHERE subject_id IS NULL;

CREATE INDEX IF NOT EXISTS idx_sc_checkpoint_events_dataset_kind_ingest_desc
    ON sc_checkpoint_events (dataset_id, kind, ingest_seq DESC);

CREATE INDEX IF NOT EXISTS idx_sc_checkpoint_events_dataset_subject_ingest_desc
    ON sc_checkpoint_events (dataset_id, subject_id, ingest_seq DESC);

CREATE INDEX IF NOT EXISTS idx_sc_checkpoint_events_stream_sequence_desc
    ON sc_checkpoint_events (dataset_id, publisher, subject_id, sequence DESC);

CREATE TABLE IF NOT EXISTS sc_proof_manifests (
    manifest_id BIGSERIAL PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    scheme_id TEXT NOT NULL DEFAULT 'SC-CHECKPOINT-V1',
    window_start_ingest_seq BIGINT NOT NULL,
    window_end_ingest_seq BIGINT NOT NULL,
    root_sha256 TEXT NOT NULL,
    manifest_sha256 TEXT,
    events_uri TEXT,
    proofs_uri TEXT,
    anomalies_uri TEXT,
    anchor_chain TEXT,
    anchor_txid TEXT,
    anchor_block_height BIGINT,
    events_count BIGINT NOT NULL DEFAULT 0,
    subjects_count BIGINT NOT NULL DEFAULT 0,
    anomaly_summary_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_proof_manifests_dataset_scheme_window_unique UNIQUE (
        dataset_id,
        scheme_id,
        window_start_ingest_seq,
        window_end_ingest_seq
    ),
    CONSTRAINT sc_proof_manifests_manifest_dataset_unique UNIQUE (manifest_id, dataset_id),

    CONSTRAINT sc_proof_manifests_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_proof_manifests_scheme_id_not_empty CHECK (btrim(scheme_id) <> ''),
    CONSTRAINT sc_proof_manifests_root_not_empty CHECK (btrim(root_sha256) <> ''),

    CONSTRAINT sc_proof_manifests_window_bounds_valid CHECK (
        window_start_ingest_seq > 0
        AND window_end_ingest_seq >= window_start_ingest_seq
    ),

    CONSTRAINT sc_proof_manifests_root_shape CHECK (root_sha256 ~* '^[0-9a-f]{64}$'),
    CONSTRAINT sc_proof_manifests_manifest_hash_shape CHECK (
        manifest_sha256 IS NULL OR manifest_sha256 ~* '^[0-9a-f]{64}$'
    ),

    CONSTRAINT sc_proof_manifests_anchor_chain_not_blank CHECK (
        anchor_chain IS NULL OR btrim(anchor_chain) <> ''
    ),
    CONSTRAINT sc_proof_manifests_anchor_txid_not_blank CHECK (
        anchor_txid IS NULL OR btrim(anchor_txid) <> ''
    ),
    CONSTRAINT sc_proof_manifests_anchor_height_non_negative CHECK (
        anchor_block_height IS NULL OR anchor_block_height >= 0
    ),

    CONSTRAINT sc_proof_manifests_counts_non_negative CHECK (
        events_count >= 0
        AND subjects_count >= 0
    ),

    CONSTRAINT sc_proof_manifests_anomaly_summary_object CHECK (
        jsonb_typeof(anomaly_summary_json) = 'object'
    )
);

CREATE INDEX IF NOT EXISTS idx_sc_proof_manifests_dataset_window_end_desc
    ON sc_proof_manifests (dataset_id, window_end_ingest_seq DESC);

CREATE INDEX IF NOT EXISTS idx_sc_proof_manifests_dataset_created_desc
    ON sc_proof_manifests (dataset_id, created_at DESC);

CREATE TABLE IF NOT EXISTS sc_event_proofs (
    proof_id BIGSERIAL PRIMARY KEY,
    manifest_id BIGINT NOT NULL,
    dataset_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    proof_position BIGINT NOT NULL,
    proof_scheme TEXT NOT NULL DEFAULT 'SC-CHECKPOINT-V1',
    proof_path_json JSONB NOT NULL,
    proof_hash TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_event_proofs_manifest_event_unique UNIQUE (manifest_id, event_id),

    CONSTRAINT sc_event_proofs_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_event_proofs_event_id_not_empty CHECK (btrim(event_id) <> ''),
    CONSTRAINT sc_event_proofs_proof_scheme_not_empty CHECK (btrim(proof_scheme) <> ''),

    CONSTRAINT sc_event_proofs_event_id_shape CHECK (event_id ~* '^[0-9a-f]{64}$'),
    CONSTRAINT sc_event_proofs_proof_hash_shape CHECK (
        proof_hash IS NULL OR proof_hash ~* '^[0-9a-f]{64}$'
    ),

    CONSTRAINT sc_event_proofs_position_non_negative CHECK (proof_position >= 0),
    CONSTRAINT sc_event_proofs_path_json_array CHECK (jsonb_typeof(proof_path_json) = 'array'),

    CONSTRAINT fk_sc_event_proofs_manifest_dataset
        FOREIGN KEY (manifest_id, dataset_id)
        REFERENCES sc_proof_manifests (manifest_id, dataset_id)
        ON DELETE RESTRICT,
    CONSTRAINT fk_sc_event_proofs_event
        FOREIGN KEY (dataset_id, event_id)
        REFERENCES sc_checkpoint_events (dataset_id, event_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sc_event_proofs_dataset_event
    ON sc_event_proofs (dataset_id, event_id);

CREATE INDEX IF NOT EXISTS idx_sc_event_proofs_dataset_position
    ON sc_event_proofs (dataset_id, proof_position);

CREATE TABLE IF NOT EXISTS sc_verification_runs (
    verification_run_id BIGSERIAL PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    manifest_id BIGINT NOT NULL,
    verifier_id TEXT NOT NULL,
    verification_scope TEXT NOT NULL DEFAULT 'window',
    target_event_id TEXT,
    status TEXT NOT NULL,
    verified_root_sha256 TEXT,
    mismatch_reason TEXT,
    verification_details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_verification_runs_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_verification_runs_verifier_id_not_empty CHECK (btrim(verifier_id) <> ''),
    CONSTRAINT sc_verification_runs_status_not_empty CHECK (btrim(status) <> ''),

    CONSTRAINT sc_verification_runs_target_event_shape CHECK (
        target_event_id IS NULL OR target_event_id ~* '^[0-9a-f]{64}$'
    ),
    CONSTRAINT sc_verification_runs_verified_root_shape CHECK (
        verified_root_sha256 IS NULL OR verified_root_sha256 ~* '^[0-9a-f]{64}$'
    ),

    CONSTRAINT sc_verification_runs_scope_valid CHECK (
        verification_scope IN ('window', 'event')
    ),
    CONSTRAINT sc_verification_runs_status_valid CHECK (
        status IN ('pending', 'verified', 'failed')
    ),

    CONSTRAINT sc_verification_runs_scope_shape CHECK (
        (verification_scope = 'window' AND target_event_id IS NULL)
        OR (verification_scope = 'event' AND target_event_id IS NOT NULL)
    ),

    CONSTRAINT sc_verification_runs_details_object CHECK (
        jsonb_typeof(verification_details_json) = 'object'
    ),

    CONSTRAINT fk_sc_verification_runs_manifest_dataset
        FOREIGN KEY (manifest_id, dataset_id)
        REFERENCES sc_proof_manifests (manifest_id, dataset_id)
        ON DELETE RESTRICT,
    CONSTRAINT fk_sc_verification_runs_target_event
        FOREIGN KEY (dataset_id, target_event_id)
        REFERENCES sc_checkpoint_events (dataset_id, event_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sc_verification_runs_dataset_verifier_checked_desc
    ON sc_verification_runs (dataset_id, verifier_id, checked_at DESC);

CREATE INDEX IF NOT EXISTS idx_sc_verification_runs_dataset_manifest_checked_desc
    ON sc_verification_runs (dataset_id, manifest_id, checked_at DESC);

CREATE TABLE IF NOT EXISTS sc_verification_state (
    dataset_id TEXT NOT NULL,
    verifier_id TEXT NOT NULL,
    last_verification_run_id BIGINT NOT NULL,
    last_manifest_id BIGINT NOT NULL,
    last_scope TEXT NOT NULL,
    last_target_event_id TEXT,
    last_status TEXT NOT NULL,
    last_checked_at TIMESTAMPTZ NOT NULL,
    last_verified_root_sha256 TEXT,
    mismatch_reason TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_verification_state_pk PRIMARY KEY (dataset_id, verifier_id),
    CONSTRAINT sc_verification_state_last_run_unique UNIQUE (last_verification_run_id),

    CONSTRAINT sc_verification_state_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_verification_state_verifier_id_not_empty CHECK (btrim(verifier_id) <> ''),
    CONSTRAINT sc_verification_state_last_scope_valid CHECK (last_scope IN ('window', 'event')),
    CONSTRAINT sc_verification_state_last_status_valid CHECK (
        last_status IN ('pending', 'verified', 'failed')
    ),
    CONSTRAINT sc_verification_state_last_scope_shape CHECK (
        (last_scope = 'window' AND last_target_event_id IS NULL)
        OR (last_scope = 'event' AND last_target_event_id IS NOT NULL)
    ),
    CONSTRAINT sc_verification_state_last_target_event_shape CHECK (
        last_target_event_id IS NULL OR last_target_event_id ~* '^[0-9a-f]{64}$'
    ),
    CONSTRAINT sc_verification_state_last_verified_root_shape CHECK (
        last_verified_root_sha256 IS NULL OR last_verified_root_sha256 ~* '^[0-9a-f]{64}$'
    ),

    CONSTRAINT fk_sc_verification_state_last_run
        FOREIGN KEY (last_verification_run_id)
        REFERENCES sc_verification_runs (verification_run_id)
        ON DELETE RESTRICT,
    CONSTRAINT fk_sc_verification_state_manifest_dataset
        FOREIGN KEY (last_manifest_id, dataset_id)
        REFERENCES sc_proof_manifests (manifest_id, dataset_id)
        ON DELETE RESTRICT,
    CONSTRAINT fk_sc_verification_state_last_event
        FOREIGN KEY (dataset_id, last_target_event_id)
        REFERENCES sc_checkpoint_events (dataset_id, event_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sc_verification_state_dataset_status
    ON sc_verification_state (dataset_id, last_status, last_checked_at DESC);

CREATE TABLE IF NOT EXISTS sc_subject_state (
    dataset_id TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    last_event_id TEXT,
    last_ingest_seq BIGINT,
    last_sequence BIGINT,
    last_checkpoint_type TEXT,
    last_checkpoint_at TIMESTAMPTZ,
    integrity_status TEXT NOT NULL DEFAULT 'ok',
    anomaly_count BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_subject_state_pk PRIMARY KEY (dataset_id, subject_id),

    CONSTRAINT sc_subject_state_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_subject_state_subject_id_not_empty CHECK (btrim(subject_id) <> ''),
    CONSTRAINT sc_subject_state_last_event_shape CHECK (
        last_event_id IS NULL OR last_event_id ~* '^[0-9a-f]{64}$'
    ),
    CONSTRAINT sc_subject_state_checkpoint_type_not_blank CHECK (
        last_checkpoint_type IS NULL OR btrim(last_checkpoint_type) <> ''
    ),
    CONSTRAINT sc_subject_state_ingest_seq_positive CHECK (
        last_ingest_seq IS NULL OR last_ingest_seq > 0
    ),
    CONSTRAINT sc_subject_state_sequence_non_negative CHECK (
        last_sequence IS NULL OR last_sequence >= 0
    ),
    CONSTRAINT sc_subject_state_integrity_status_valid CHECK (
        integrity_status IN ('ok', 'gap', 'anomaly')
    ),
    CONSTRAINT sc_subject_state_anomaly_count_non_negative CHECK (anomaly_count >= 0),

    CONSTRAINT fk_sc_subject_state_last_event
        FOREIGN KEY (dataset_id, last_event_id)
        REFERENCES sc_checkpoint_events (dataset_id, event_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sc_subject_state_dataset_integrity_updated_desc
    ON sc_subject_state (dataset_id, integrity_status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_sc_subject_state_dataset_last_ingest_desc
    ON sc_subject_state (dataset_id, last_ingest_seq DESC);

CREATE TABLE IF NOT EXISTS sc_anomalies (
    anomaly_row_id BIGSERIAL PRIMARY KEY,
    dataset_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    subject_id TEXT,
    anomaly_code TEXT NOT NULL,
    severity TEXT NOT NULL DEFAULT 'warn',
    reason_code TEXT,
    details_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    ingest_seq BIGINT NOT NULL,
    observed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    CONSTRAINT sc_anomalies_dataset_event_code_unique UNIQUE (dataset_id, event_id, anomaly_code),

    CONSTRAINT sc_anomalies_dataset_id_not_empty CHECK (btrim(dataset_id) <> ''),
    CONSTRAINT sc_anomalies_event_id_not_empty CHECK (btrim(event_id) <> ''),
    CONSTRAINT sc_anomalies_anomaly_code_not_empty CHECK (btrim(anomaly_code) <> ''),
    CONSTRAINT sc_anomalies_severity_not_empty CHECK (btrim(severity) <> ''),
    CONSTRAINT sc_anomalies_reason_code_not_blank CHECK (
        reason_code IS NULL OR btrim(reason_code) <> ''
    ),
    CONSTRAINT sc_anomalies_subject_id_not_blank CHECK (
        subject_id IS NULL OR btrim(subject_id) <> ''
    ),

    CONSTRAINT sc_anomalies_event_id_shape CHECK (event_id ~* '^[0-9a-f]{64}$'),
    CONSTRAINT sc_anomalies_severity_valid CHECK (severity IN ('info', 'warn', 'error')),
    CONSTRAINT sc_anomalies_ingest_seq_positive CHECK (ingest_seq > 0),
    CONSTRAINT sc_anomalies_details_object CHECK (jsonb_typeof(details_json) = 'object'),

    CONSTRAINT fk_sc_anomalies_event
        FOREIGN KEY (dataset_id, event_id)
        REFERENCES sc_checkpoint_events (dataset_id, event_id)
        ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_sc_anomalies_dataset_subject_ingest_desc
    ON sc_anomalies (dataset_id, subject_id, ingest_seq DESC);

CREATE INDEX IF NOT EXISTS idx_sc_anomalies_dataset_code_ingest_desc
    ON sc_anomalies (dataset_id, anomaly_code, ingest_seq DESC);

CREATE OR REPLACE FUNCTION sc_block_update_delete()
RETURNS trigger AS $$
BEGIN
    RAISE EXCEPTION '% is append-only; % is not allowed', TG_TABLE_NAME, TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sc_checkpoint_events_enforce_invariants()
RETURNS trigger AS $$
DECLARE
    last_dataset_ingest_seq BIGINT;
    last_stream_sequence BIGINT;
    predecessor_publisher TEXT;
    predecessor_subject_id TEXT;
    predecessor_sequence BIGINT;
    predecessor_ingest_seq BIGINT;
    last_stream_checkpoint_at TIMESTAMPTZ;
BEGIN
    -- Serialize inserts per dataset so monotonic checks hold under concurrent writers.
    PERFORM pg_advisory_xact_lock(hashtext(NEW.dataset_id)::bigint);

    SELECT ingest_seq
    INTO last_dataset_ingest_seq
    FROM sc_checkpoint_events
    WHERE dataset_id = NEW.dataset_id
    ORDER BY ingest_seq DESC
    LIMIT 1;

    IF last_dataset_ingest_seq IS NOT NULL AND NEW.ingest_seq <= last_dataset_ingest_seq THEN
        RAISE EXCEPTION
            'ingest_seq must increase monotonically per dataset (dataset_id=%, new=%, last=%)',
            NEW.dataset_id,
            NEW.ingest_seq,
            last_dataset_ingest_seq;
    END IF;

    SELECT sequence
    INTO last_stream_sequence
    FROM sc_checkpoint_events
    WHERE dataset_id = NEW.dataset_id
      AND publisher = NEW.publisher
      AND subject_id IS NOT DISTINCT FROM NEW.subject_id
    ORDER BY sequence DESC
    LIMIT 1;

    IF last_stream_sequence IS NOT NULL AND NEW.sequence <= last_stream_sequence THEN
        RAISE EXCEPTION
            'sequence must increase monotonically per stream (dataset_id=%, publisher=%, subject_id=%, new=%, last=%)',
            NEW.dataset_id,
            NEW.publisher,
            COALESCE(NEW.subject_id, '<null>'),
            NEW.sequence,
            last_stream_sequence;
    END IF;

    IF NEW.stream_prev_event_id IS NOT NULL THEN
        SELECT publisher, subject_id, sequence, ingest_seq
        INTO predecessor_publisher, predecessor_subject_id, predecessor_sequence, predecessor_ingest_seq
        FROM sc_checkpoint_events
        WHERE dataset_id = NEW.dataset_id
          AND event_id = NEW.stream_prev_event_id;

        IF NOT FOUND THEN
            RAISE EXCEPTION
                'stream_prev_event_id % was not found in dataset %',
                NEW.stream_prev_event_id,
                NEW.dataset_id;
        END IF;

        IF predecessor_publisher <> NEW.publisher THEN
            RAISE EXCEPTION
                'stream_prev_event_id % publisher mismatch (expected %, found %)',
                NEW.stream_prev_event_id,
                predecessor_publisher,
                NEW.publisher;
        END IF;

        IF predecessor_subject_id IS DISTINCT FROM NEW.subject_id THEN
            RAISE EXCEPTION
                'stream_prev_event_id % subject mismatch (expected %, found %)',
                NEW.stream_prev_event_id,
                COALESCE(predecessor_subject_id, '<null>'),
                COALESCE(NEW.subject_id, '<null>');
        END IF;

        IF NEW.sequence <= predecessor_sequence THEN
            RAISE EXCEPTION
                'sequence must be greater than predecessor sequence (new=%, predecessor=%)',
                NEW.sequence,
                predecessor_sequence;
        END IF;

        IF NEW.ingest_seq <= predecessor_ingest_seq THEN
            RAISE EXCEPTION
                'ingest_seq must be greater than predecessor ingest_seq (new=%, predecessor=%)',
                NEW.ingest_seq,
                predecessor_ingest_seq;
        END IF;
    END IF;

    IF NEW.kind = 'supplychain.checkpoint.v1' THEN
        SELECT checkpoint_at
        INTO last_stream_checkpoint_at
        FROM sc_checkpoint_events
        WHERE dataset_id = NEW.dataset_id
          AND publisher = NEW.publisher
          AND subject_id IS NOT DISTINCT FROM NEW.subject_id
          AND kind = 'supplychain.checkpoint.v1'
        ORDER BY sequence DESC
        LIMIT 1;

        IF last_stream_checkpoint_at IS NOT NULL AND NEW.checkpoint_at < last_stream_checkpoint_at THEN
            RAISE EXCEPTION
                'checkpoint_at must be non-decreasing per stream (dataset_id=%, publisher=%, subject_id=%)',
                NEW.dataset_id,
                NEW.publisher,
                NEW.subject_id;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sc_event_proofs_enforce_consistency()
RETURNS trigger AS $$
DECLARE
    manifest_dataset_id TEXT;
    manifest_window_start BIGINT;
    manifest_window_end BIGINT;
    proof_event_ingest_seq BIGINT;
    proof_event_kind TEXT;
BEGIN
    SELECT dataset_id, window_start_ingest_seq, window_end_ingest_seq
    INTO manifest_dataset_id, manifest_window_start, manifest_window_end
    FROM sc_proof_manifests
    WHERE manifest_id = NEW.manifest_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'manifest_id % was not found', NEW.manifest_id;
    END IF;

    IF manifest_dataset_id <> NEW.dataset_id THEN
        RAISE EXCEPTION
            'proof dataset_id % does not match manifest dataset_id %',
            NEW.dataset_id,
            manifest_dataset_id;
    END IF;

    SELECT ingest_seq, kind
    INTO proof_event_ingest_seq, proof_event_kind
    FROM sc_checkpoint_events
    WHERE dataset_id = NEW.dataset_id
      AND event_id = NEW.event_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'event_id % was not found for dataset %',
            NEW.event_id,
            NEW.dataset_id;
    END IF;

    IF proof_event_kind <> 'supplychain.checkpoint.v1' THEN
        RAISE EXCEPTION
            'event_id % has kind % and is not eligible for sc_event_proofs',
            NEW.event_id,
            proof_event_kind;
    END IF;

    IF proof_event_ingest_seq < manifest_window_start OR proof_event_ingest_seq > manifest_window_end THEN
        RAISE EXCEPTION
            'event ingest_seq % is out of manifest window [% - %]',
            proof_event_ingest_seq,
            manifest_window_start,
            manifest_window_end;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sc_verification_runs_enforce_consistency()
RETURNS trigger AS $$
DECLARE
    manifest_root_sha256 TEXT;
    manifest_window_start BIGINT;
    manifest_window_end BIGINT;
    target_event_ingest_seq BIGINT;
BEGIN
    SELECT root_sha256, window_start_ingest_seq, window_end_ingest_seq
    INTO manifest_root_sha256, manifest_window_start, manifest_window_end
    FROM sc_proof_manifests
    WHERE manifest_id = NEW.manifest_id
      AND dataset_id = NEW.dataset_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION
            'manifest_id % for dataset_id % was not found',
            NEW.manifest_id,
            NEW.dataset_id;
    END IF;

    IF NEW.status = 'verified' THEN
        IF NEW.verified_root_sha256 IS NULL THEN
            RAISE EXCEPTION 'verified_root_sha256 is required when status is verified';
        END IF;

        IF lower(NEW.verified_root_sha256) <> lower(manifest_root_sha256) THEN
            RAISE EXCEPTION
                'verified_root_sha256 % does not match manifest root %',
                NEW.verified_root_sha256,
                manifest_root_sha256;
        END IF;
    END IF;

    IF NEW.status = 'failed' AND (NEW.mismatch_reason IS NULL OR btrim(NEW.mismatch_reason) = '') THEN
        RAISE EXCEPTION 'mismatch_reason is required when status is failed';
    END IF;

    IF NEW.verification_scope = 'event' THEN
        SELECT ingest_seq
        INTO target_event_ingest_seq
        FROM sc_checkpoint_events
        WHERE dataset_id = NEW.dataset_id
          AND event_id = NEW.target_event_id;

        IF NOT FOUND THEN
            RAISE EXCEPTION
                'target_event_id % was not found for dataset %',
                NEW.target_event_id,
                NEW.dataset_id;
        END IF;

        IF target_event_ingest_seq < manifest_window_start OR target_event_ingest_seq > manifest_window_end THEN
            RAISE EXCEPTION
                'target_event_id % ingest_seq % is out of manifest window [% - %]',
                NEW.target_event_id,
                target_event_ingest_seq,
                manifest_window_start,
                manifest_window_end;
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sc_checkpoint_events_apply_derived_models()
RETURNS trigger AS $$
DECLARE
    derived_code TEXT;
    derived_severity TEXT;
    derived_reason_code TEXT;
BEGIN
    IF NEW.kind = 'supplychain.checkpoint.v1' THEN
        INSERT INTO sc_subject_state (
            dataset_id,
            subject_id,
            last_event_id,
            last_ingest_seq,
            last_sequence,
            last_checkpoint_type,
            last_checkpoint_at,
            integrity_status,
            updated_at
        )
        VALUES (
            NEW.dataset_id,
            NEW.subject_id,
            NEW.event_id,
            NEW.ingest_seq,
            NEW.sequence,
            NEW.checkpoint_type,
            NEW.checkpoint_at,
            'ok',
            NOW()
        )
        ON CONFLICT (dataset_id, subject_id) DO UPDATE
        SET last_event_id = EXCLUDED.last_event_id,
            last_ingest_seq = EXCLUDED.last_ingest_seq,
            last_sequence = EXCLUDED.last_sequence,
            last_checkpoint_type = EXCLUDED.last_checkpoint_type,
            last_checkpoint_at = EXCLUDED.last_checkpoint_at,
            integrity_status = 'ok',
            updated_at = NOW()
        WHERE sc_subject_state.last_ingest_seq IS NULL
           OR sc_subject_state.last_ingest_seq <= EXCLUDED.last_ingest_seq;

        RETURN NULL;
    END IF;

    IF NEW.kind IN ('supplychain.gap.v1', 'supplychain.anomaly.v1') THEN
        derived_code := COALESCE(
            NULLIF(NEW.payload_json->>'code', ''),
            NULLIF(NEW.payload_json->>'reason_code', ''),
            CASE
                WHEN NEW.kind = 'supplychain.gap.v1' THEN 'SEQUENCE_GAP'
                ELSE 'UNSPECIFIED_ANOMALY'
            END
        );

        derived_severity := COALESCE(
            NULLIF(NEW.payload_json->>'severity', ''),
            CASE
                WHEN NEW.kind = 'supplychain.gap.v1' THEN 'warn'
                ELSE 'error'
            END
        );

        IF derived_severity NOT IN ('info', 'warn', 'error') THEN
            derived_severity := 'warn';
        END IF;

        derived_reason_code := NULLIF(NEW.payload_json->>'reason_code', '');

        INSERT INTO sc_anomalies (
            dataset_id,
            event_id,
            subject_id,
            anomaly_code,
            severity,
            reason_code,
            details_json,
            ingest_seq,
            observed_at
        )
        VALUES (
            NEW.dataset_id,
            NEW.event_id,
            NEW.subject_id,
            derived_code,
            derived_severity,
            derived_reason_code,
            COALESCE(NEW.payload_json->'details', '{}'::jsonb),
            NEW.ingest_seq,
            NEW.inserted_at
        )
        ON CONFLICT (dataset_id, event_id, anomaly_code) DO NOTHING;

        IF NEW.subject_id IS NOT NULL THEN
            INSERT INTO sc_subject_state (
                dataset_id,
                subject_id,
                last_event_id,
                last_ingest_seq,
                last_sequence,
                integrity_status,
                anomaly_count,
                updated_at
            )
            VALUES (
                NEW.dataset_id,
                NEW.subject_id,
                NEW.event_id,
                NEW.ingest_seq,
                NEW.sequence,
                CASE
                    WHEN NEW.kind = 'supplychain.gap.v1' THEN 'gap'
                    ELSE 'anomaly'
                END,
                1,
                NOW()
            )
            ON CONFLICT (dataset_id, subject_id) DO UPDATE
            SET integrity_status = CASE
                    WHEN NEW.kind = 'supplychain.gap.v1' THEN 'gap'
                    ELSE 'anomaly'
                END,
                anomaly_count = sc_subject_state.anomaly_count + 1,
                updated_at = NOW();
        END IF;
    END IF;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION sc_verification_runs_apply_state()
RETURNS trigger AS $$
BEGIN
    INSERT INTO sc_verification_state (
        dataset_id,
        verifier_id,
        last_verification_run_id,
        last_manifest_id,
        last_scope,
        last_target_event_id,
        last_status,
        last_checked_at,
        last_verified_root_sha256,
        mismatch_reason,
        updated_at
    )
    VALUES (
        NEW.dataset_id,
        NEW.verifier_id,
        NEW.verification_run_id,
        NEW.manifest_id,
        NEW.verification_scope,
        NEW.target_event_id,
        NEW.status,
        NEW.checked_at,
        NEW.verified_root_sha256,
        NEW.mismatch_reason,
        NOW()
    )
    ON CONFLICT (dataset_id, verifier_id) DO UPDATE
    SET last_verification_run_id = EXCLUDED.last_verification_run_id,
        last_manifest_id = EXCLUDED.last_manifest_id,
        last_scope = EXCLUDED.last_scope,
        last_target_event_id = EXCLUDED.last_target_event_id,
        last_status = EXCLUDED.last_status,
        last_checked_at = EXCLUDED.last_checked_at,
        last_verified_root_sha256 = EXCLUDED.last_verified_root_sha256,
        mismatch_reason = EXCLUDED.mismatch_reason,
        updated_at = NOW()
    WHERE sc_verification_state.last_checked_at <= EXCLUDED.last_checked_at;

    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_sc_checkpoint_events_invariants ON sc_checkpoint_events;
CREATE TRIGGER trg_sc_checkpoint_events_invariants
BEFORE INSERT ON sc_checkpoint_events
FOR EACH ROW EXECUTE FUNCTION sc_checkpoint_events_enforce_invariants();

DROP TRIGGER IF EXISTS trg_sc_checkpoint_events_apply_derived_models ON sc_checkpoint_events;
CREATE TRIGGER trg_sc_checkpoint_events_apply_derived_models
AFTER INSERT ON sc_checkpoint_events
FOR EACH ROW EXECUTE FUNCTION sc_checkpoint_events_apply_derived_models();

DROP TRIGGER IF EXISTS trg_sc_event_proofs_consistency ON sc_event_proofs;
CREATE TRIGGER trg_sc_event_proofs_consistency
BEFORE INSERT ON sc_event_proofs
FOR EACH ROW EXECUTE FUNCTION sc_event_proofs_enforce_consistency();

DROP TRIGGER IF EXISTS trg_sc_verification_runs_consistency ON sc_verification_runs;
CREATE TRIGGER trg_sc_verification_runs_consistency
BEFORE INSERT ON sc_verification_runs
FOR EACH ROW EXECUTE FUNCTION sc_verification_runs_enforce_consistency();

DROP TRIGGER IF EXISTS trg_sc_verification_runs_apply_state ON sc_verification_runs;
CREATE TRIGGER trg_sc_verification_runs_apply_state
AFTER INSERT ON sc_verification_runs
FOR EACH ROW EXECUTE FUNCTION sc_verification_runs_apply_state();

DROP TRIGGER IF EXISTS trg_sc_checkpoint_events_no_update_delete ON sc_checkpoint_events;
CREATE TRIGGER trg_sc_checkpoint_events_no_update_delete
BEFORE UPDATE OR DELETE ON sc_checkpoint_events
FOR EACH ROW EXECUTE FUNCTION sc_block_update_delete();

DROP TRIGGER IF EXISTS trg_sc_proof_manifests_no_update_delete ON sc_proof_manifests;
CREATE TRIGGER trg_sc_proof_manifests_no_update_delete
BEFORE UPDATE OR DELETE ON sc_proof_manifests
FOR EACH ROW EXECUTE FUNCTION sc_block_update_delete();

DROP TRIGGER IF EXISTS trg_sc_event_proofs_no_update_delete ON sc_event_proofs;
CREATE TRIGGER trg_sc_event_proofs_no_update_delete
BEFORE UPDATE OR DELETE ON sc_event_proofs
FOR EACH ROW EXECUTE FUNCTION sc_block_update_delete();

DROP TRIGGER IF EXISTS trg_sc_verification_runs_no_update_delete ON sc_verification_runs;
CREATE TRIGGER trg_sc_verification_runs_no_update_delete
BEFORE UPDATE OR DELETE ON sc_verification_runs
FOR EACH ROW EXECUTE FUNCTION sc_block_update_delete();
