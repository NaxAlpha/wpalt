<?php
// Synthetic, disposable reference only; this file is not part of wpalt runtime.
require '/var/www/html/wp-load.php';
require_once '/var/www/html/wp-admin/includes/plugin.php';
if (get_option('wpalt_reference_seeded')) { throw new RuntimeException('Reference fixture already seeded; use a new disposable site.'); }
if (!function_exists('acf_add_local_field_group')) { throw new RuntimeException('Install and activate ACF first.'); }
update_option('wpalt_reference_seeded', true);
update_option('permalink_structure','/%postname%/');flush_rewrite_rules();
wp_suspend_cache_addition(true);
update_option('posts_per_page',20);
$draft=wp_insert_post(['post_title'=>'Reference draft','post_content'=>'PRIVATE_REFERENCE_DRAFT','post_status'=>'draft']);
$published=wp_insert_post(['post_title'=>'Reference publishing journey','post_content'=>'PUBLIC_REFERENCE_CONTENT','post_status'=>'publish']);
wp_update_post(['ID'=>$published,'post_content'=>'UPDATED_REFERENCE_CONTENT']);
$revisions=wp_get_post_revisions($published);
acf_add_local_field_group(['key'=>'group_wpalt_ref','title'=>'Reference fields','fields'=>[
 ['key'=>'field_wpalt_subtitle','label'=>'Subtitle','name'=>'subtitle','type'=>'text'],
 ['key'=>'field_wpalt_featured','label'=>'Featured','name'=>'featured','type'=>'true_false']
], 'location'=>[[['param'=>'post_type','operator'=>'==','value'=>'post']]]]);
update_field('field_wpalt_subtitle','Reference subtitle',$published);
update_field('field_wpalt_featured',true,$published);
$names=[['A quieter place on the web','Our digital spaces should feel like places we own. A little slower, more considered, and built around the stories we want to tell.'],['Notes from the garden','Good things take root with a little patience. These are the things we have been making, reading and learning.'],['Building with intention','A small website can do a lot. Start with clear words, thoughtful structure and tools that stay out of the way.'],['A field guide to independent publishing','Own your words, your audience and your archives. Publishing should not require a collection of accounts to keep your site running.']];
for($i=0;$i<1000;$i++){
 [$title,$lead]=$names[$i%4];
 $body="<p>$lead</p><h2>Room to think</h2><p>This is a publishing example: structured content, a shared theme and a publishing workflow you can run on your own server.</p><ul><li>Draft and preview before publishing.</li><li>Keep unfinished changes away from your live pages.</li><li>Back up your content and take it with you.</li></ul><blockquote>Useful tools should make good work easier.</blockquote><h3>What comes next</h3><p>Explore the administration panel, edit this story, and switch themes.</p>";
 wp_insert_post(['post_title'=>$title.' · '.($i+1),'post_content'=>$body,'post_status'=>'publish','post_name'=>'journal-'.($i+1)]);
}
echo json_encode(['wordpress_version'=>$wp_version,'acf_version'=>ACF_VERSION,'ase_version'=>get_plugin_data('/var/www/html/wp-content/plugins/admin-site-enhancements/admin-site-enhancements.php')['Version'],'draft_id'=>$draft,'published_id'=>$published,'revision_count'=>count($revisions),'acf_subtitle'=>get_field('subtitle',$published),'acf_featured'=>(bool)get_field('featured',$published),'published_posts'=>(int)wp_count_posts()->publish,'peak_cli_php_bytes'=>memory_get_peak_usage(true)])."\n";
