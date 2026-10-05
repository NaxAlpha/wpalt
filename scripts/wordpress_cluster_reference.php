<?php
// Original synthetic reference fixture, operated only in disposable WordPress.
wp_set_current_user(get_user_by('login','owner')->ID);
require_once ABSPATH.'wp-admin/includes/plugin.php';
function wpalt_reference_version($domain){foreach(get_plugins() as $plugin){if(($plugin['TextDomain']??'')===$domain)return $plugin['Version'];}throw new Exception('Reference plugin version missing');}
$api=\MailPoet\API\API::MP('v1');
$reader=$api->addSubscriber(['email'=>'reference-reader@example.test','first_name'=>'Reference','last_name'=>'Reader'],[],['send_confirmation_email'=>false]);
$reader=$api->getSubscriber('reference-reader@example.test');
if(empty($reader['id'])) throw new Exception('MailPoet reference subscriber missing');
$level=new PMPro_Membership_Level();$level->name='Reference members';$level->initial_payment=0;$level->billing_amount=0;$level->cycle_number=0;$level->cycle_period='Month';$level->billing_limit=0;$level->trial_amount=0;$level->trial_limit=0;$level->expiration_number=0;$level->expiration_period='';$level->allow_signups=0;$level->save();
$levels=pmpro_getAllLevels(true,false,true);$selected=[];foreach($levels as $l){if($l->name==='Reference members')$selected[]=['id'=>(string)$l->id,'name'=>$l->name];}if(count($selected)!==1)throw new Exception('PMPro reference level missing');
$course=wp_insert_post(['post_type'=>'course','post_title'=>'Reference learning','post_status'=>'publish']);
$lessons=[];foreach(['First reference lesson','Second reference lesson'] as $i=>$title){$lesson=wp_insert_post(['post_type'=>'lesson','post_title'=>$title,'post_content'=>'<p>A reference lesson.</p>','post_status'=>'publish']);update_post_meta($lesson,'_lesson_course',$course);update_post_meta($lesson,'_order_'.$course,$i+1);}
foreach(Sensei()->course->course_lessons($course) as $lesson){$lessons[]=['id'=>(string)$lesson->ID,'title'=>$lesson->post_title,'content'=>$lesson->post_content];}
if(count($lessons)!==2||$lessons[0]['title']!=='First reference lesson')throw new Exception('Sensei reference ordering mismatch');
$product=new WC_Product_Simple();$product->set_name('Reference kit');$product->set_slug('reference-kit');$product->set_description('A physical reference kit.');$product->set_sku('M8-KIT');$product->set_regular_price('12.34');$product->set_manage_stock(true);$product->set_stock_quantity(4);$product->set_backorders('no');$product->set_status('draft');$product_id=$product->save();$product=wc_get_product($product_id);
if($product->get_regular_price()!=='12.34'||$product->get_stock_quantity()!==4)throw new Exception('WooCommerce reference catalog mismatch');
$data=['format'=>'wpalt-plugin-clusters-v1','source_site'=>get_option('siteurl'),
'mailpoet'=>['version'=>wpalt_reference_version('mailpoet'),'subscribers'=>[['id'=>(string)$reader['id'],'email'=>$reader['email'],'name'=>trim($reader['first_name'].' '.$reader['last_name']),'status'=>$reader['status']]]],
'pmpro'=>['version'=>wpalt_reference_version('paid-memberships-pro'),'levels'=>$selected],
'sensei'=>['version'=>wpalt_reference_version('sensei-lms'),'courses'=>[['id'=>(string)$course,'title'=>get_the_title($course),'lessons'=>$lessons]]],
'woocommerce'=>['version'=>WC_VERSION,'currency'=>get_woocommerce_currency(),'products'=>[['id'=>(string)$product->get_id(),'name'=>$product->get_name(),'slug'=>$product->get_slug(),'description'=>$product->get_description(),'sku'=>$product->get_sku(),'regular_price'=>$product->get_regular_price(),'stock_quantity'=>$product->get_manage_stock()?$product->get_stock_quantity():null,'product_type'=>$product->get_type(),'virtual_product'=>$product->get_virtual(),'downloadable'=>$product->get_downloadable(),'backorders'=>$product->get_backorders()]]]];
file_put_contents('/var/www/html/wpalt-m8-clusters.json',json_encode($data));
echo json_encode(['contacts'=>1,'membership_policies'=>1,'courses'=>1,'lessons'=>2,'products'=>1]);
