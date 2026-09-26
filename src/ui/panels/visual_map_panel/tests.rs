use super::VisualMapPanelEvent;

#[test]
fn test_visual_map_panel_event_navigate_to() {
    let event = VisualMapPanelEvent::NavigateTo { offset: 0x1234 };
    assert_eq!(event, VisualMapPanelEvent::NavigateTo { offset: 0x1234 });
    match event {
        VisualMapPanelEvent::NavigateTo { offset } => assert_eq!(offset, 0x1234),
    }
}
