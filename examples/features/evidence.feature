Feature: evidence on demand

  Scenario: a screenshot from a passing step
    Given I am on "/login"
    And I am in debug mode
    When I take a screenshot
    Then the "text=Sign in" element should be visible

  Scenario: the page source and one element, for writing a selector
    Given I am on "/login"
    And I am in debug mode
    When I dump the DOM
    And I dump the DOM of "h1"
    Then the "text=Sign in" element should be visible

  Scenario: a second dump of a kind gets the next number
    Given I am on "/login"
    And I am in debug mode
    When I take a screenshot
    And I take a screenshot
    And I dump the DOM
    And I dump the DOM
    Then the "text=Sign in" element should be visible
