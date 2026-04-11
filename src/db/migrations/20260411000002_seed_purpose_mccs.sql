-- Seed data: Health purpose MCCs
INSERT INTO purpose_mcc_allowlist (purpose_code, mcc, mcc_description) VALUES
    ('health', '5912', 'Drug stores and pharmacies'),
    ('health', '8011', 'Doctors'),
    ('health', '8021', 'Dentists and orthodontists'),
    ('health', '8031', 'Osteopaths'),
    ('health', '8041', 'Chiropractors'),
    ('health', '8042', 'Optometrists and ophthalmologists'),
    ('health', '8049', 'Podiatrists and chiropodists'),
    ('health', '8062', 'Hospitals'),
    ('health', '8071', 'Medical and dental laboratories'),
    ('health', '8099', 'Medical services and health practitioners');

-- Seed data: Education purpose MCCs
INSERT INTO purpose_mcc_allowlist (purpose_code, mcc, mcc_description) VALUES
    ('education', '5111', 'Stationery, office supplies'),
    ('education', '5192', 'Books, periodicals, and newspapers'),
    ('education', '5942', 'Bookstores'),
    ('education', '5943', 'Stationery stores'),
    ('education', '8211', 'Elementary and secondary schools'),
    ('education', '8220', 'Colleges and universities'),
    ('education', '8241', 'Correspondence schools'),
    ('education', '8244', 'Business and secretarial schools'),
    ('education', '8249', 'Vocational and trade schools'),
    ('education', '8299', 'Schools and educational services');

-- Seed data: Food purpose MCCs
INSERT INTO purpose_mcc_allowlist (purpose_code, mcc, mcc_description) VALUES
    ('food', '5411', 'Grocery stores and supermarkets'),
    ('food', '5412', 'Meat provisioners — freezer and locker'),
    ('food', '5422', 'Freezer and locker meat provisioners'),
    ('food', '5441', 'Candy, nut, and confectionery stores'),
    ('food', '5451', 'Dairy product stores'),
    ('food', '5462', 'Bakeries'),
    ('food', '5499', 'Miscellaneous food stores'),
    ('food', '5812', 'Eating places and restaurants'),
    ('food', '5813', 'Bars, cocktail lounges, nightclubs'),
    ('food', '5814', 'Fast food restaurants');

-- Seed data: Transport purpose MCCs
INSERT INTO purpose_mcc_allowlist (purpose_code, mcc, mcc_description) VALUES
    ('transport', '4011', 'Railroads'),
    ('transport', '4111', 'Local and suburban commuter transit'),
    ('transport', '4112', 'Passenger railways'),
    ('transport', '4121', 'Taxicabs and rideshares'),
    ('transport', '4131', 'Bus lines'),
    ('transport', '4214', 'Motor freight carriers and trucking'),
    ('transport', '4411', 'Steamship and cruise lines'),
    ('transport', '4511', 'Airlines and air carriers'),
    ('transport', '4789', 'Transportation services'),
    ('transport', '7512', 'Automobile rental agency');
