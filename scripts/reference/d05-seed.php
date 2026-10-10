<?php
// Synthetic API-authored documents; no plugin implementation is copied.
wp_set_current_user(get_user_by('login', 'owner')->ID);
update_option('permalink_structure', '/%postname%/');
flush_rewrite_rules();
$legacy = [['id'=>'legacyroot','elType'=>'container','settings'=>['flex_direction'=>'column','gap'=>['unit'=>'px','size'=>24]],'elements'=>[
    ['id'=>'legacyheading','elType'=>'widget','widgetType'=>'heading','settings'=>['title'=>'D05_LEGACY_HEADING','header_size'=>'h1','title_color'=>'#24594f','typography_font_size'=>['unit'=>'px','size'=>32]],'elements'=>[]],
    ['id'=>'legacycopy','elType'=>'widget','widgetType'=>'text-editor','settings'=>['editor'=>'<p>D05_LEGACY_BODY with <strong>useful</strong> content.</p>'],'elements'=>[]],
    ['id'=>'legacylink','elType'=>'widget','widgetType'=>'button','settings'=>['text'=>'D05_LEGACY_LINK','link'=>['url'=>'/local-path']],'elements'=>[]]
]]];
$atomic = [['id'=>'atomicroot','elType'=>'e-div-block','version'=>'0.0','settings'=>[],'styles'=>[],'interactions'=>[],'editor_settings'=>[],'elements'=>[
    ['id'=>'atomicheading','elType'=>'widget','widgetType'=>'e-heading','version'=>'0.0','settings'=>['title'=>['$$type'=>'escaped-html','value'=>'D05_ATOMIC_HEADING'],'tag'=>['$$type'=>'string','value'=>'h1']],'styles'=>[],'interactions'=>[],'editor_settings'=>[],'elements'=>[]],
    ['id'=>'atomiccopy','elType'=>'widget','widgetType'=>'e-paragraph','version'=>'0.0','settings'=>['paragraph'=>['$$type'=>'string','value'=>'<p>D05_ATOMIC_BODY.</p>']],'styles'=>[],'interactions'=>[],'editor_settings'=>[],'elements'=>[]],
    ['id'=>'atomicbutton','elType'=>'widget','widgetType'=>'e-button','version'=>'0.0','settings'=>['text'=>['$$type'=>'escaped-html','value'=>'D05_ATOMIC_LINK'],'link'=>['$$type'=>'link','value'=>['destination'=>['$$type'=>'url','value'=>'/local-path'],'tag'=>['$$type'=>'string','value'=>'a']]]],'styles'=>[],'interactions'=>[],'editor_settings'=>[],'elements'=>[]]
]]];
$summary = [];
foreach (['legacy'=>$legacy, 'atomic'=>$atomic] as $name=>$elements) {
    $id = wp_insert_post(['post_type'=>'page','post_title'=>'D05 '.$name,'post_name'=>'d05-'.$name,'post_status'=>'publish']);
    $document = \Elementor\Plugin::$instance->documents->get($id);
    $document->set_is_built_with_elementor(true);
    if (!$document->save(['elements'=>$elements,'settings'=>['post_status'=>'publish']])) {
        throw new Exception('Reference document save failed');
    }
    $export = $document->get_export_data();
    file_put_contents('/var/www/html/wpalt-d05-'.$name.'.json', wp_json_encode([
        'title'=>'D05 '.$name,'type'=>'page','version'=>'0.4',
        'page_settings'=>$export['settings'],'metadata'=>$export['metadata'],'content'=>$export['content']
    ]));
    $summary[$name] = ['id'=>$id,'elements'=>count($document->get_elements_data()),'url'=>get_permalink($id)];
}
$widgets = array_keys(\Elementor\Plugin::$instance->widgets_manager->get_widget_types());
sort($widgets);
echo wp_json_encode(['documents'=>$summary,'registered_widget_names'=>$widgets]);
