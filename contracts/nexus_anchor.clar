;; Clarity 4 Pilot Anchor
;; [CON-1200] Verification Test

(define-constant ERR_UNAUTHORIZED (err u100))
(define-data-var contract-owner principal tx-sender)
(define-data-var last-state-root (buff 32) 0x0000000000000000000000000000000000000000000000000000000000000000)

(define-public (update-state-root (new-root (buff 32)))
    (begin
        (asserts! (is-eq tx-sender (var-get contract-owner)) ERR_UNAUTHORIZED)
        (ok (var-set last-state-root new-root))
    )
)

(define-read-only (get-last-state-root)
    (ok (var-get last-state-root))
)

(define-public (set-owner (new-owner principal))
    (begin
        (asserts! (is-eq tx-sender (var-get contract-owner)) ERR_UNAUTHORIZED)
        (ok (var-set contract-owner new-owner))
    )
)
