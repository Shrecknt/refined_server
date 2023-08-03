-- Database: chest_storage

DROP TABLE IF EXISTS chest_items;
DROP TABLE IF EXISTS chests;

CREATE TABLE chests (
	x FLOAT NOT NULL,
	y FLOAT NOT NULL,
	Z FLOAT NOT NULL,
	UNIQUE (x, y, z)
);

CREATE TABLE chest_items (
	chest_item_id BIGINT GENERATED ALWAYS AS IDENTITY,
	x FLOAT NOT NULL,
	y FLOAT NOT NULL,
	z FLOAT NOT NULL,
	location_in_chest INT NOT NULL,
	PRIMARY KEY(chest_item_id),
	CONSTRAINT fk_chest
		FOREIGN KEY(x, y, z) 
		REFERENCES chests(x, y, z),
	item_id TEXT NOT NULL,
	item_count SMALLINT NOT NULL,
	item_nbt BYTEA NOT NULL,
	UNIQUE (x, y, z, location_in_chest)
);

INSERT INTO chests (x, y, z) VALUES (0, 0, 0) ON CONFLICT (x, y, z) DO NOTHING;

DROP FUNCTION IF EXISTS insert_item_into_chest;
CREATE OR REPLACE PROCEDURE insert_item_into_chest (
	_x float,
	_y float,
	_z float,
	_location_in_chest int,
	_item_id text,
	_item_count smallint,
	_item_nbt bytea
) AS $$
	BEGIN
		INSERT INTO chest_items (x, y, z, location_in_chest, item_id, item_count, item_nbt)
		VALUES (_x, _y, _z, _location_in_chest, _item_id, _item_count, _item_nbt)
		ON CONFLICT (x, y, z, location_in_chest)
		DO UPDATE SET
			item_id = excluded.item_id,
			item_count = excluded.item_count,
			item_nbt = excluded.item_nbt;
	END;
$$ LANGUAGE plpgsql;

DROP FUNCTION IF EXISTS get_items_from_chest;
CREATE OR REPLACE FUNCTION get_items_from_chest(
	_x float,
	_y float,
	_z float
) RETURNS TABLE (
	chest_item_id BIGINT,
	x FLOAT,
	y FLOAT,
	z FLOAT,
	location_in_chest INT,
	item_id TEXT,
	item_count SMALLINT,
	item_nbt BYTEA
) AS $$
	BEGIN
		RETURN QUERY SELECT chest_items.* FROM chest_items JOIN chests
			ON (chest_items.x = chests.x AND
				chest_items.y = chests.y AND
				chest_items.z = chests.z)
			WHERE chests.x = _x AND chests.y = _y AND chests.z = _z;
	END;
$$ LANGUAGE plpgsql;

CALL insert_item_into_chest (0::float, 0::float, 0::float, 2::int, 'minecraft:gravel'::text, 32::smallint, ''::bytea);

INSERT INTO chest_items (x, y, z, location_in_chest, item_id, item_count, item_nbt)
	VALUES (0, 0, 0, 0, 'minecraft:stone', 64, ''::bytea)
	ON CONFLICT (x, y, z, location_in_chest)
	DO UPDATE SET
		item_id = excluded.item_id,
		item_count = excluded.item_count,
		item_nbt = excluded.item_nbt;
		
INSERT INTO chest_items (x, y, z, location_in_chest, item_id, item_count, item_nbt)
	VALUES (0, 0, 0, 0, 'minecraft:diamond_block', 64, ''::bytea)
	ON CONFLICT (x, y, z, location_in_chest)
	DO UPDATE SET
		item_id = excluded.item_id,
		item_count = excluded.item_count,
		item_nbt = excluded.item_nbt;
		
INSERT INTO chest_items (x, y, z, location_in_chest, item_id, item_count, item_nbt)
	VALUES (0, 0, 0, 1, 'minecraft:grass_block', 64, ''::bytea)
	ON CONFLICT (x, y, z, location_in_chest)
	DO UPDATE SET
		item_id = excluded.item_id,
		item_count = excluded.item_count,
		item_nbt = excluded.item_nbt;

SELECT * FROM chests;
SELECT * FROM chest_items;

SELECT chest_items.* FROM chest_items JOIN chests
	ON (chest_items.x = chests.x AND
		chest_items.y = chests.y AND
		chest_items.z = chests.z)
	WHERE chests.x = 0 AND chests.y = 0 AND chests.z = 0;
	
SELECT * FROM get_items_from_chest(0::float, 0::float, 0::float);
