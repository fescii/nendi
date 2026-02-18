-- Seed data for integration tests

INSERT INTO public.customers (name, email, status) VALUES
    ('Alice Nakamura', 'alice@example.com', 'active'),
    ('Bob Wanjiku',    'bob@example.com',   'active'),
    ('Carol Odhiambo', 'carol@example.com', 'suspended')
ON CONFLICT (email) DO NOTHING;

INSERT INTO public.orders (customer, status, amount, currency, items) VALUES
    (1, 'completed', 25000, 'KES', '[{"sku": "SKU-001", "qty": 2}]'),
    (1, 'pending',   15000, 'KES', '[{"sku": "SKU-002", "qty": 1}]'),
    (2, 'completed', 50000, 'KES', '[{"sku": "SKU-003", "qty": 5}]'),
    (3, 'failed',     8000, 'KES', '[{"sku": "SKU-001", "qty": 1}]');

INSERT INTO public.payments (order_id, amount, method, status) VALUES
    (1, 25000, 'mpesa',  'completed'),
    (3, 50000, 'card',   'completed'),
    (4,  8000, 'mpesa',  'failed');
