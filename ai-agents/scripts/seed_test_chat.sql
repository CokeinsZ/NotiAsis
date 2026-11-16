-- Chat de prueba del agente de atención al cliente.
--
-- Crea (o reinicia) un business de pruebas con el agente activado y un chat
-- con el teléfono de prueba, como si NotiAsis ya le hubiera notificado una
-- guía que está en oficina: video + mensaje_guia_es hace 3 días,
-- recordatorio hace 2, recordatorio_final ayer y pedido_oficina hace 1 hora.
--
-- Es idempotente: cada vez que se corre borra la conversación del chat de
-- prueba (incluidas las respuestas del agente, escalamientos y novedades) y
-- la deja recién notificada. No toca ningún otro business.
--
--   psql "$DATABASE_URL" -f ai-agents/scripts/seed_test_chat.sql
--
-- Cambia test_phone para probar con otro número (no debe ser de un asesor:
-- el bot ignora los textos de los asesores).

DO $$
DECLARE
	test_phone CONSTANT VARCHAR := '573003579384';
	test_name CONSTANT VARCHAR := 'Stiven Carvajal';
	test_business CONSTANT VARCHAR := 'Victor Store Pruebas';
	guide CONSTANT VARCHAR := '999000000001';  -- prefijo 999: nunca choca con una guía real
	product CONSTANT VARCHAR := 'EXTRACTO PIMIENTA DE CAYENA 3000 Mg';
	address CONSTANT VARCHAR := 'Oficina Interrapidisimo';
	city CONSTANT VARCHAR := 'BOGOTA';
	-- Último aviso hace 1 hora; los anteriores, un día antes cada uno.
	last_at CONSTANT TIMESTAMP := (now() AT TIME ZONE 'UTC') - interval '1 hour';
	bid INT;
BEGIN
	SELECT id INTO bid FROM businesses WHERE name = test_business;
	IF bid IS NULL THEN
		INSERT INTO businesses (name, country_id)
			SELECT test_business, id FROM countries WHERE code = 'CO'
			RETURNING id INTO bid;
	END IF;

	-- Políticas de EJEMPLO, solo para probar: reemplázalas por las reales.
	INSERT INTO business_ai_agents (business_id, enabled, agent_name, policies, human_takeover_hours)
	VALUES (bid, true, 'Verónica',
		'La transportadora guarda el pedido 8 días en la oficina; después lo devuelve. '
		'Se paga en efectivo al reclamarlo (contra entrega). '
		'No se puede cambiar la dirección de un pedido ya despachado.',
		12)
	ON CONFLICT (business_id) DO UPDATE SET enabled = true, updated_at = CURRENT_TIMESTAMP;

	INSERT INTO users (phone_number, full_name) VALUES (test_phone, test_name)
	ON CONFLICT (phone_number) DO NOTHING;

	-- Limpiar la conversación anterior del chat de prueba.
	DELETE FROM messages WHERE business_id = bid AND user_id = test_phone;
	DELETE FROM ai_escalations WHERE business_id = bid AND user_id = test_phone;
	DELETE FROM guide_notifications WHERE guide_number = guide;
	DELETE FROM guides WHERE number = guide;  -- borra también productos y novedades

	INSERT INTO chats (business_id, user_id, last_guide_notification_at)
	VALUES (bid, test_phone, last_at)
	ON CONFLICT (business_id, user_id) DO UPDATE SET
		last_user_message = NULL,
		last_user_message_timestamp = NULL,
		is_important = false,
		last_guide_notification_at = EXCLUDED.last_guide_notification_at;

	-- Sin order_id: la herramienta de estado en vivo responde que no hay
	-- pedido de Dropi (la guía es inventada).
	INSERT INTO guides (number, user_id, business_id, customer_name, delivery_address, city,
	                    shipping_company, order_total, currency, notification_count, last_notification_timestamp)
	VALUES (guide, test_phone, bid, test_name, address, city, 'INTERRAPIDISIMO', 79900, 'COP', 4, last_at);

	INSERT INTO guide_products (guide_number, product_id, product_name, quantity)
	VALUES (guide, 2240718, product, 1);

	INSERT INTO guide_notifications (guide_number, business_id, user_id, notification_step, kind, meta_message_id, created_at)
	VALUES
		(guide, bid, test_phone, 1, 'office', 'wamid.PRUEBA_IA_2', last_at - interval '3 days'),
		(guide, bid, test_phone, 2, 'office', 'wamid.PRUEBA_IA_3', last_at - interval '2 days'),
		(guide, bid, test_phone, 3, 'office', 'wamid.PRUEBA_IA_4', last_at - interval '1 day'),
		(guide, bid, test_phone, 4, 'office', 'wamid.PRUEBA_IA_5', last_at);

	-- Los textos son los de las plantillas (log_message() de app/whatsapp/templates/).
	INSERT INTO messages (meta_message_id, business_id, user_id, media_type, message, status, from_user,
	                      created_at, guide_number, sender)
	VALUES
		('wamid.PRUEBA_IA_1', bid, test_phone, 'video', NULL, 'read', false,
			last_at - interval '3 days' - interval '1 second', NULL, 'bot'),
		('wamid.PRUEBA_IA_2', bid, test_phone, 'text',
			'Es hora de recoger tu pedido' || E'\n' ||
			'Hola ' || test_name || ',' || E'\n\n' ||
			'Tu pedido ' || guide || ' con el producto ' || product || ', ya está listo para recoger en ' ||
			address || ', ' || city || '.' || E'\n\n' ||
			'Por favor reclamar lo antes posible para evitar devoluciones por parte de la empresa transportadora.' ||
			E'\n\n' || '¡Disfruta!',
			'read', false, last_at - interval '3 days', guide, 'bot'),
		('wamid.PRUEBA_IA_3', bid, test_phone, 'text',
			'Tu pedido te espera' || E'\n' || 'Hola' || E'\n\n' ||
			'Te escribimos para recordarte que tu pedido ya esta listo para recoger en las oficinas en tu ciudad ' ||
			'de la transportadora. ' || E'\n\n' || '¡Que disfrutes tu compra!',
			'read', false, last_at - interval '2 days', guide, 'bot'),
		('wamid.PRUEBA_IA_4', bid, test_phone, 'text',
			'Reclama tu pedido' || E'\n' ||
			'Hola ' || test_name || ', ¡tu ' || product || E'\n' || 'está a punto de ser devuelto!' || E'\n\n' ||
			'Te notificamos que el plazo máximo para reclamar tu ' || E'\n' ||
			'paquete en la oficina de la transportadora está llegando ' || E'\n' || 'a su fin. ' || E'\n\n' ||
			'Por favor pasa a recogerlo a la brevedad posible para que ' || E'\n' ||
			'no sea retornado a nuestro almacén.' || E'\n',
			'read', false, last_at - interval '1 day', guide, 'bot'),
		('wamid.PRUEBA_IA_5', bid, test_phone, 'text',
			'Hola ' || test_name || ' !!😀' || E'\n\n' ||
			'Hablas con Veronica ¿Cómo estas?, estaba verificando y tu pedido de ' || product ||
			' ya está en la oficina para ser reclamado.' || E'\n\n' ||
			'El  número de guia es: ' || guide || E'\n' ||
			'En  la oficina de: ' || address || ', ' || city || E'\n' ||
			'Y debes pagar: *$79.900*' || E'\n\n' ||
			'¿Hoy lo puedes recoger? Quedo atenta ' || E'\n' || 'Bendiciones',
			'read', false, last_at, guide, 'bot');

	RAISE NOTICE 'Chat de prueba listo: business_id=%, teléfono=%, guía=%', bid, test_phone, guide;
END $$;
