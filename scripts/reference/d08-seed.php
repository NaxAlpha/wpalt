<?php
// Synthetic fixture using the pinned free plugin's own installation/model APIs.
$installed = \Redirection\Database\Database::get_latest_database()->install();
if (is_wp_error($installed)) { throw new RuntimeException('Reference installation failed'); }
$group = Red_Group::create('D08 reference', WordPress_Module::MODULE_ID);
if (is_wp_error($group) || !$group) { throw new RuntimeException('Reference group failed'); }
$group_id = $group->get_id();
$options = red_set_options(['monitor_post'=>$group_id, 'monitor_types'=>['post'], 'expire_redirect'=>7, 'expire_404'=>7, 'ip_logging'=>0, 'log_header'=>false]);
$monitor = new Red_Monitor($options);
$id = wp_insert_post(['post_type'=>'post','post_status'=>'publish','post_title'=>'Reference story','post_name'=>'source-old','post_content'=>'Actual local reference publication.'], true);
if (is_wp_error($id)) { throw new RuntimeException('Reference publication failed'); }
$before = wp_parse_url(get_permalink($id), PHP_URL_PATH);
$updated = wp_update_post(['ID'=>$id,'post_name'=>'source-new'],true);
if (is_wp_error($updated)) { throw new RuntimeException('Reference publication update failed'); }
$after = wp_parse_url(get_permalink($id), PHP_URL_PATH);
foreach ([
 ['url'=>'/exact-old','target'=>$after,'regex'=>false,'code'=>301],
 ['url'=>'^/legacy/([a-z0-9-]+)$','target'=>'/$1/','regex'=>true,'code'=>302],
 ['url'=>'/partial/','target'=>'/new-part/','regex'=>true,'code'=>301],
 ['url'=>'^/except/(?!skip)(.*)$','target'=>$after,'regex'=>true,'code'=>301],
] as $rule) {
 $item=Red_Item::create(['url'=>$rule['url'],'regex'=>$rule['regex'],'action_data'=>['url'=>$rule['target']],'match_type'=>'url','action_type'=>'url','action_code'=>$rule['code'],'group_id'=>$group_id]);
 if (is_wp_error($item)) { throw new RuntimeException('Reference rule creation failed'); }
}
echo wp_json_encode(['story_id'=>$id,'group_id'=>$group_id,'before'=>$before,'after'=>$after]);
