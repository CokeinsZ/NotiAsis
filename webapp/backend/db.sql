CREATE TYPE state_enum AS ENUM (
	'Active',
	'Inactive',
	'Blocked',
	'Deleted'
);

CREATE TYPE media_type_enum AS ENUM (
	'text',
	'document',
	'audio',
	'image'
);

CREATE TYPE message_status_enum AS ENUM (
	'sent',
	'delivered',
	'read'	
);

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

CREATE TABLE IF NOT EXISTS chats (
	business_id INT NOT NULL,
	user_id VARCHAR(20) NOT NULL,
	last_user_message_timestamp TIMESTAMP,
	last_user_message TEXT,

	PRIMARY KEY (business_id, user_id),

	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE,
	FOREIGN KEY (user_id) REFERENCES users(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS messages (
	id SERIAL PRIMARY KEY,
	meta_message_id VARCHAR(255) UNIQUE NOT NULL,
	business_id INT NOT NULL,
	user_id VARCHAR(20) NOT NULL,
	media_id VARCHAR(255),
	media_type media_type_enum NOT NULL DEFAULT 'text',
	message TEXT,
	status message_status_enum NOT NULL DEFAULT 'sent',
	from_user BOOLEAN NOT NULL DEFAULT FALSE,
	created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,

	FOREIGN KEY (business_id) REFERENCES businesses(id) 
		ON DELETE CASCADE
		ON UPDATE CASCADE,
	FOREIGN KEY (user_id) REFERENCES users(id) 
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

CREATE INDEX idx_messages_chat_time ON messages (business_id, user_id, created_at DESC);
CREATE INDEX idx_messages_meta_id ON messages (meta_message_id);