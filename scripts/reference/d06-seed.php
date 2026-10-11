<?php
// Synthetic fixture through public WordPress/Yoast APIs; no plugin implementation copied.
require_once ABSPATH . 'wp-admin/includes/image.php';
$upload = wp_upload_bits('d06-source.png', null, file_get_contents(ABSPATH . 'wpalt-d06-image.png'));
if (!empty($upload['error'])) { throw new RuntimeException('Reference image admission failed'); }
$attachment = wp_insert_attachment(['post_mime_type'=>'image/png','post_title'=>'D06 garden','post_status'=>'inherit'], $upload['file']);
if (is_wp_error($attachment)) { throw new RuntimeException('Reference attachment creation failed'); }
wp_update_attachment_metadata($attachment, wp_generate_attachment_metadata($attachment, $upload['file']));
$url = wp_get_attachment_url($attachment);
$post = wp_insert_post(['post_title'=>'D06 actual journal','post_name'=>'d06-actual-journal','post_content'=>'A relevant editorial image. <img src="'.esc_url($url).'" alt="D06 garden" />','post_status'=>'publish','post_type'=>'post'], true);
if (is_wp_error($post)) { throw new RuntimeException('Reference publication failed'); }
update_post_meta($post, '_yoast_wpseo_schema_article_type', 'Article');
update_post_meta($post, '_yoast_wpseo_title', 'D06 truthful search title');
update_post_meta($post, '_yoast_wpseo_metadesc', 'D06 actual publication description');
update_post_meta($post, '_yoast_wpseo_opengraph-image', $url);
update_post_meta($post, '_yoast_wpseo_opengraph-image-id', $attachment);
YoastSEO()->classes->get(\Yoast\WP\SEO\Builders\Indexable_Builder::class)->build_for_id_and_type($post, 'post');
echo wp_json_encode(['id'=>$post,'factual_title'=>get_the_title($post),'url'=>get_permalink($post),'image'=>$url,'title'=>'D06 truthful search title','description'=>'D06 actual publication description']);
