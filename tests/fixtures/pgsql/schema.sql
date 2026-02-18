-- Nendi test schema
-- Tables used by integration tests

CREATE TABLE IF NOT EXISTS public.customers (
    id        BIGSERIAL PRIMARY KEY,
    name      TEXT      NOT NULL,
    email     TEXT      NOT NULL UNIQUE,
    status    TEXT      NOT NULL DEFAULT 'active',
    meta      JSONB     NOT NULL DEFAULT '{}',
    created   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.orders (
    id        BIGSERIAL PRIMARY KEY,
    customer  BIGINT    NOT NULL REFERENCES public.customers(id),
    status    TEXT      NOT NULL DEFAULT 'pending',
    amount    BIGINT    NOT NULL DEFAULT 0,
    currency  TEXT      NOT NULL DEFAULT 'USD',
    items     JSONB     NOT NULL DEFAULT '[]',
    created   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS public.payments (
    id        BIGSERIAL PRIMARY KEY,
    order_id  BIGINT    NOT NULL REFERENCES public.orders(id),
    amount    BIGINT    NOT NULL,
    method    TEXT      NOT NULL DEFAULT 'card',
    status    TEXT      NOT NULL DEFAULT 'pending',
    created   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for order lookups by customer
CREATE INDEX IF NOT EXISTS idx_orders_customer ON public.orders(customer);

-- Index for payment lookups by order
CREATE INDEX IF NOT EXISTS idx_payments_order ON public.payments(order_id);
