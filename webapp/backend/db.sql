DO $$ BEGIN
	CREATE TYPE state_enum AS ENUM (
		'Active',
		'Inactive',
		'Blocked',
		'Deleted'
	);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
	CREATE TYPE media_type_enum AS ENUM (
		'text',
		'document',
		'audio',
		'image',
		'video'
	);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

DO $$ BEGIN
	CREATE TYPE message_status_enum AS ENUM (
		'sent',
		'delivered',
		'read'
	);
EXCEPTION WHEN duplicate_object THEN NULL;
END $$;

CREATE TABLE IF NOT EXISTS users (
	phone_number VARCHAR(20) PRIMARY KEY,
	full_name VARCHAR(255) NOT NULL
);

CREATE TABLE IF NOT EXISTS businesses (
	id SERIAL PRIMARY KEY,
	name VARCHAR(255) NOT NULL,
	state state_enum NOT NULL DEFAULT 'Active',

	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
	updated_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS business_associates (
	id SERIAL PRIMARY KEY,
	business_id INT NOT NULL,
	phone_number VARCHAR(20) NOT NULL,
	username VARCHAR(255) UNIQUE NOT NULL,
	password_hash VARCHAR(255) NOT NULL,

	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS business_users_sheet (
	id SERIAL PRIMARY KEY,
	business_id INT NOT NULL,
	document_id VARCHAR(64) NOT NULL,
	office_id VARCHAR(12),
	delivered_id VARCHAR(12),

	FOREIGN KEY (business_id) REFERENCES businesses(id)
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS chats (
	business_id INT NOT NULL,
	user_id VARCHAR(20) NOT NULL,
	last_user_message_timestamp TIMESTAMP,
	last_user_message TEXT,

	is_important BOOLEAN NOT NULL DEFAULT FALSE,
	last_guide_notification_at TIMESTAMP,

	PRIMARY KEY (business_id, user_id),

	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE,
	FOREIGN KEY (user_id) REFERENCES users(phone_number) 
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS messages (
	id SERIAL PRIMARY KEY,
	meta_message_id VARCHAR(255) UNIQUE NOT NULL,
	business_id INT NOT NULL,  -- Foreign key to chats table
	user_id VARCHAR(20) NOT NULL,  -- Foreign key to chats table
	media_id VARCHAR(255),
	media_type media_type_enum NOT NULL DEFAULT 'text',
	message TEXT,
	status message_status_enum DEFAULT 'sent',
	from_user BOOLEAN NOT NULL DEFAULT FALSE,
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE,
	FOREIGN KEY (user_id) REFERENCES users(phone_number) 
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS guides (
	number VARCHAR(20) PRIMARY KEY,
	user_id VARCHAR(20) NOT NULL,
    business_id INT NOT NULL,
	last_notification_timestamp TIMESTAMP,
	notification_count INT NOT NULL DEFAULT 0,

	FOREIGN KEY (user_id) REFERENCES users(phone_number)
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

-- Datos del pedido asociados a la guía (cliente y productos). Se guardan
-- la primera vez que se notifica un pedido de Dropi para que los
-- recordatorios no vuelvan a consultar el MCP (tiene rate limit).
-- ALTER en vez de columnas en el CREATE para migrar bases existentes.
ALTER TABLE guides ADD COLUMN IF NOT EXISTS order_id VARCHAR(32);
ALTER TABLE guides ADD COLUMN IF NOT EXISTS customer_name VARCHAR(255);
ALTER TABLE guides ADD COLUMN IF NOT EXISTS customer_document VARCHAR(32);
ALTER TABLE guides ADD COLUMN IF NOT EXISTS delivery_address TEXT;
ALTER TABLE guides ADD COLUMN IF NOT EXISTS city VARCHAR(128);
ALTER TABLE guides ADD COLUMN IF NOT EXISTS shipping_company VARCHAR(128);
ALTER TABLE guides ADD COLUMN IF NOT EXISTS order_total NUMERIC(14, 2);
ALTER TABLE guides ADD COLUMN IF NOT EXISTS currency VARCHAR(8);

CREATE TABLE IF NOT EXISTS guide_products (
	id SERIAL PRIMARY KEY,
	guide_number VARCHAR(20) NOT NULL,
	product_id BIGINT,
	product_name VARCHAR(512) NOT NULL,
	quantity INT NOT NULL DEFAULT 1,
	unit_price NUMERIC(14, 2),
	subtotal NUMERIC(14, 2),

	FOREIGN KEY (guide_number) REFERENCES guides(number)
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS invoices (
	id SERIAL PRIMARY KEY,
	business_id INT NOT NULL,
	amount DECIMAL(10, 2) NOT NULL,
	status state_enum NOT NULL DEFAULT 'Active',
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE SET NULL
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS api_keys (
	id SERIAL PRIMARY KEY,
	key VARCHAR(255) UNIQUE NOT NULL,
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS guide_notifications (
	id SERIAL PRIMARY KEY,
	guide_number VARCHAR(20),
	business_id INT NOT NULL,
	user_id VARCHAR(20) NOT NULL,
	notification_step INT NOT NULL,
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

	FOREIGN KEY (guide_number) REFERENCES guides(number) 
		ON DELETE SET NULL
		ON UPDATE CASCADE,
	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE,
	FOREIGN KEY (user_id) REFERENCES users(phone_number) 
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_chat_time ON messages (business_id, user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_chats_business_activity ON chats (business_id, last_user_message_timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_guide_notifications_business_date ON guide_notifications (business_id, (created_at::date));
CREATE INDEX IF NOT EXISTS idx_guide_notifications_guide ON guide_notifications (guide_number);
CREATE INDEX IF NOT EXISTS idx_guide_products_guide ON guide_products (guide_number);