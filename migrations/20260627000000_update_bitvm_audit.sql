-- [CON-1313] Update bitvm_verified_transitions to support full proof auditing
ALTER TABLE bitvm_verified_transitions ADD COLUMN IF NOT EXISTS vk_hash TEXT;
ALTER TABLE bitvm_verified_transitions ADD COLUMN IF NOT EXISTS public_inputs_hash TEXT;
