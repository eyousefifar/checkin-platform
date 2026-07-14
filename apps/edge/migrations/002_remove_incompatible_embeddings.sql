DELETE FROM employee_embeddings
WHERE model_name <> 'buffalo_l' OR num_images_used < 5;
